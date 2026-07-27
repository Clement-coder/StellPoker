import { describe, it, expect, vi, beforeEach } from "vitest";

/**
 * A wallet seated at several tables at once (#72) can fire actions from two
 * tables in the same tick. Each Stellar transaction is signed against the
 * account's current sequence number and the network accepts exactly one
 * transaction per sequence, so unsynchronised submissions would read the same
 * sequence and the second would be rejected with `txBAD_SEQ`.
 *
 * These tests drive `onchain.ts` against a fake RPC whose sequence only
 * advances once a transaction has been sent, and whose `getAccount` is slow
 * enough that an unqueued implementation would definitely collide.
 */

/** Sequence number currently on the ledger, per account. */
const ledgerSequences = new Map<string, number>();
/** Sequence each submission actually built against, in submission order. */
const builtWith: { address: string; sequence: number }[] = [];

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 5));
}

vi.mock("@/lib/api", () => ({
  getChainConfig: vi.fn().mockResolvedValue({
    rpc_url: "https://soroban-testnet.stellar.org",
    network_passphrase: "Test SDF Network ; September 2015",
    poker_table_contract: "CCONTRACT123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ",
  }),
}));

vi.mock("@stellar/stellar-sdk", () => {
  class FakeServer {
    async getAccount(address: string) {
      // Slow enough that two unqueued callers would both read the same value.
      await tick();
      const sequence = ledgerSequences.get(address) ?? 1;
      return { address, sequence };
    }
    prepareTransaction = vi.fn().mockImplementation(async (tx: unknown) => tx);
    async sendTransaction(tx: { address: string; sequence: number }) {
      await tick();
      // The ledger only advances once the transaction has been accepted.
      ledgerSequences.set(tx.address, tx.sequence + 1);
      return { status: "SUCCESS", hash: `hash-${tx.address}-${tx.sequence}` };
    }
    pollTransaction = vi.fn().mockResolvedValue({ status: "SUCCESS" });
  }

  class FakeTransactionBuilder {
    private account: { address: string; sequence: number };
    constructor(account: { address: string; sequence: number }) {
      this.account = account;
    }
    addOperation() {
      return this;
    }
    setTimeout() {
      return this;
    }
    build() {
      builtWith.push({ ...this.account });
      return {
        ...this.account,
        toXDR: () => `${this.account.address}:${this.account.sequence}`,
      };
    }
    static fromXDR(xdrString: string) {
      const [address, sequence] = xdrString.split(":");
      return { address, sequence: Number(sequence) };
    }
  }

  return {
    rpc: {
      Server: FakeServer,
      Api: { GetTransactionStatus: { SUCCESS: "SUCCESS", FAILED: "FAILED" } },
    },
    TransactionBuilder: FakeTransactionBuilder,
    Contract: class {
      call = vi.fn().mockReturnValue("operation");
    },
    Address: class {
      constructor(private addr: string) {}
      toScVal = () => `scval_${this.addr}`;
    },
    nativeToScVal: (val: unknown) => `scval_${String(val)}`,
    BASE_FEE: "100",
    xdr: {
      ScVal: {
        scvSymbol: (s: string) => s,
        scvVec: (v: unknown[]) => v,
      },
    },
  };
});

const HERO = "GHERO";
const VILLAIN = "GVILLAIN";

function walletFor(address: string) {
  return {
    address,
    walletType: "lobstr" as const,
    signMessage: vi.fn().mockResolvedValue("sig"),
  };
}

beforeEach(() => {
  ledgerSequences.clear();
  builtWith.length = 0;
  // Route signing through the Lobstr branch, which needs no dynamic import.
  (window as unknown as { lobstr: unknown }).lobstr = {
    signTransaction: (txXdr: string) => Promise.resolve({ signedTxXdr: txXdr }),
  };
});

describe("on-chain submission sequencing", () => {
  it("gives parallel submissions from one wallet distinct sequence numbers", async () => {
    const { playerActionOnChain } = await import("@/lib/onchain");
    const wallet = walletFor(HERO);

    // Two tables acting in the same tick.
    await Promise.all([
      playerActionOnChain(wallet, 1, "fold"),
      playerActionOnChain(wallet, 2, "check"),
      playerActionOnChain(wallet, 3, "call"),
    ]);

    const sequences = builtWith
      .filter((entry) => entry.address === HERO)
      .map((entry) => entry.sequence);

    expect(sequences).toHaveLength(3);
    expect(new Set(sequences).size).toBe(3);
    // Each submission picked up the sequence the previous one left behind.
    expect(sequences).toEqual([1, 2, 3]);
  });

  it("does not serialize different wallets against each other", async () => {
    const { playerActionOnChain } = await import("@/lib/onchain");

    await Promise.all([
      playerActionOnChain(walletFor(HERO), 1, "fold"),
      playerActionOnChain(walletFor(VILLAIN), 2, "fold"),
    ]);

    // Each account starts from its own sequence rather than queueing behind
    // the other.
    expect(builtWith.filter((e) => e.address === HERO)).toEqual([
      { address: HERO, sequence: 1 },
    ]);
    expect(builtWith.filter((e) => e.address === VILLAIN)).toEqual([
      { address: VILLAIN, sequence: 1 },
    ]);
  });

  it("keeps the queue moving after a failed submission", async () => {
    const { playerActionOnChain } = await import("@/lib/onchain");
    const wallet = walletFor(HERO);

    // A rejected signature must not poison every later submission.
    const lobstr = (window as unknown as {
      lobstr: { signTransaction: (xdr: string) => Promise<unknown> };
    }).lobstr;
    const original = lobstr.signTransaction;
    let calls = 0;
    lobstr.signTransaction = (txXdr: string) => {
      calls += 1;
      return calls === 1
        ? Promise.resolve({ error: "user rejected" })
        : original(txXdr);
    };

    const results = await Promise.allSettled([
      playerActionOnChain(wallet, 1, "fold"),
      playerActionOnChain(wallet, 2, "fold"),
    ]);

    expect(results[0].status).toBe("rejected");
    expect(results[1].status).toBe("fulfilled");
  });
});
