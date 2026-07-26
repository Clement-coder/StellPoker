#!/usr/bin/env python3
"""Test the full MPC poker flow with on-chain Soroban integration.

Flow: deal → preflop betting → flop → flop betting → turn → turn betting →
      river → river betting → showdown

On-chain betting is done via `stellar contract invoke` calls between MPC phases.
"""

import json
import os
import struct
import subprocess
import time
import requests
from nacl.signing import SigningKey

DEFAULT_ENV_FILE = os.environ.get(
    "TEST_ENV_FILE",
    os.path.join(os.path.dirname(__file__), "..", ".env.local"),
)

env_vars = {}
if os.path.exists(DEFAULT_ENV_FILE):
    with open(DEFAULT_ENV_FILE) as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                env_vars[k] = v.strip('"')

BASE = os.environ.get(
    "COORDINATOR_URL",
    env_vars.get("NEXT_PUBLIC_COORDINATOR_URL", "http://localhost:8080"),
).rstrip("/")
SOROBAN_RPC = os.environ.get(
    "SOROBAN_RPC",
    env_vars.get("SOROBAN_RPC", "http://localhost:8000/soroban/rpc"),
)
NETWORK_PASSPHRASE = os.environ.get(
    "NETWORK_PASSPHRASE",
    env_vars.get("NETWORK_PASSPHRASE", "Standalone Network ; February 2017"),
)
COMMITTEE_IDENTITY = os.environ.get(
    "COMMITTEE_IDENTITY",
    env_vars.get("COMMITTEE_IDENTITY", "committee-local"),
)
POKER_TABLE_CONTRACT = env_vars.get("POKER_TABLE_CONTRACT", "")
PLAYER1_ADDRESS = env_vars.get("PLAYER1_ADDRESS", "")
PLAYER2_ADDRESS = env_vars.get("PLAYER2_ADDRESS", "")
PLAYER1_IDENTITY = os.environ.get(
    "PLAYER1_IDENTITY",
    env_vars.get("PLAYER1_IDENTITY", "player1-local"),
)
PLAYER2_IDENTITY = os.environ.get(
    "PLAYER2_IDENTITY",
    env_vars.get("PLAYER2_IDENTITY", "player2-local"),
)
TABLE_ID = int(env_vars.get("TABLE_ID", os.environ.get("TABLE_ID", "0")))
ON_CHAIN = bool(POKER_TABLE_CONTRACT)

print(f"Using env file: {DEFAULT_ENV_FILE}")
print(f"Coordinator URL: {BASE}")

if ON_CHAIN:
    print(f"On-chain mode: contract={POKER_TABLE_CONTRACT}")
    print(f"  Table ID: {TABLE_ID}")
    print(f"  Soroban RPC: {SOROBAN_RPC}")
    print(f"  Player 1: {PLAYER1_ADDRESS}")
    print(f"  Player 2: {PLAYER2_ADDRESS}")
else:
    print("Off-chain mode (no POKER_TABLE_CONTRACT configured)")

# --- Stellar key helpers ---

def encode_stellar_pubkey(raw_32: bytes) -> str:
    """Encode raw ed25519 public key as Stellar G... address."""
    payload = bytes([6 << 3]) + raw_32
    crc = _crc16_xmodem(payload)
    full = payload + struct.pack("<H", crc)
    return _base32_encode(full)

def _crc16_xmodem(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            if crc & 0x8000:
                crc = (crc << 1) ^ 0x1021
            else:
                crc <<= 1
            crc &= 0xFFFF
    return crc

def _base32_encode(data: bytes) -> str:
    import base64
    return base64.b32encode(data).decode("ascii").rstrip("=")

# --- Auth helpers ---

def make_auth_headers(signing_key: SigningKey, address: str, table_id: int, action: str, nonce: int) -> dict:
    timestamp = int(time.time())
    message = f"stellar-poker|{address}|{table_id}|{action}|{nonce}|{timestamp}"
    sig = signing_key.sign(message.encode()).signature
    return {
        "x-player-address": address,
        "x-auth-signature": sig.hex(),
        "x-auth-nonce": str(nonce),
        "x-auth-timestamp": str(timestamp),
        "Content-Type": "application/json",
    }

# --- On-chain betting helpers ---

def stellar_player_action(player_identity: str, table_id: int, player_address: str, action_json: str):
    """Call player_action on the poker-table contract."""
    if not ON_CHAIN:
        return True
    cmd = [
        "stellar", "contract", "invoke",
        "--id", POKER_TABLE_CONTRACT,
        "--source", player_identity,
        "--rpc-url", SOROBAN_RPC,
        "--network-passphrase", NETWORK_PASSPHRASE,
        "--",
        "player_action",
        "--table_id", str(table_id),
        "--player", player_address,
        "--action", action_json,
    ]
    print(f"    stellar: player_action({player_identity}, {action_json})")
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    if result.returncode != 0:
        print(f"    ERROR: {result.stderr.strip()}")
        return False
    print(f"    OK")
    return True

def get_on_chain_phase():
    """Read on-chain table phase."""
    state = get_on_chain_table()
    if isinstance(state, dict):
        return state.get("phase", "unknown")
    return state

def get_on_chain_table():
    """Read full on-chain table state."""
    if not ON_CHAIN:
        return None
    cmd = [
        "stellar", "contract", "invoke",
        "--id", POKER_TABLE_CONTRACT,
        "--source", COMMITTEE_IDENTITY,
        "--rpc-url", SOROBAN_RPC,
        "--network-passphrase", NETWORK_PASSPHRASE,
        "--send=no",
        "--",
        "get_table",
        "--table_id", str(TABLE_ID),
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
    if result.returncode == 0:
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return "parse_error"
    return "error"

def player_stack_total(table_state: dict) -> int:
    total = 0
    for player in table_state.get("players", []):
        total += int(player.get("stack", 0))
    return total

def table_chip_total(table_state: dict) -> int:
    return (
        player_stack_total(table_state)
        + int(table_state.get("pot", 0))
        + int(table_state.get("rake_balance", 0))
    )

def ensure_on_chain_ready_for_deal():
    """Ensure on-chain table is in Dealing phase before requesting MPC deal."""
    if not ON_CHAIN:
        return
    phase = get_on_chain_phase()
    if phase == "Dealing":
        return
    if phase in ("Waiting", "Settlement"):
        print(f"  On-chain phase is {phase}; starting a new hand...")
        cmd = [
            "stellar", "contract", "invoke",
            "--id", POKER_TABLE_CONTRACT,
            "--source", COMMITTEE_IDENTITY,
            "--rpc-url", SOROBAN_RPC,
            "--network-passphrase", NETWORK_PASSPHRASE,
            "--",
            "start_hand",
            "--table_id", str(TABLE_ID),
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode != 0:
            print(f"  ERROR: failed to start hand: {result.stderr.strip()}")
            exit(1)
        phase = get_on_chain_phase()
        if phase != "Dealing":
            print(f"  ERROR: expected Dealing after start_hand, got {phase}")
            exit(1)
        return
    print(f"  ERROR: on-chain table is in unexpected phase {phase}; cannot request new deal safely")
    exit(1)

def do_preflop_betting():
    """Both players complete preflop betting: SB calls, BB checks."""
    print("\n=== On-Chain Betting: Preflop ===")
    phase = get_on_chain_phase()
    print(f"  Phase: {phase}")
    if phase != "Preflop":
        print(f"  ERROR: expected Preflop phase, got {phase}")
        exit(1)
    # Seat 0 (player1 = SB) calls to match BB
    if not stellar_player_action(PLAYER1_IDENTITY, TABLE_ID, PLAYER1_ADDRESS, '"Call"'):
        exit(1)
    phase = get_on_chain_phase()
    # In heads-up with current contract logic, this may already advance the phase.
    if phase == "Preflop":
        if not stellar_player_action(PLAYER2_IDENTITY, TABLE_ID, PLAYER2_ADDRESS, '"Check"'):
            exit(1)
        phase = get_on_chain_phase()
    phase = get_on_chain_phase()
    print(f"  Phase after betting: {phase}")

def do_postflop_betting(round_name: str):
    """Both players check through a post-flop betting round."""
    print(f"\n=== On-Chain Betting: {round_name} ===")
    phase = get_on_chain_phase()
    print(f"  Phase: {phase}")
    expected = round_name  # "Flop", "Turn", or "River"
    if phase != expected:
        print(f"  ERROR: expected {expected} phase, got {phase}")
        exit(1)
    # Post-flop: seat (dealer+1)%2 = 0 acts first
    if not stellar_player_action(PLAYER1_IDENTITY, TABLE_ID, PLAYER1_ADDRESS, '"Check"'):
        exit(1)
    phase = get_on_chain_phase()
    if phase == expected:
        if not stellar_player_action(PLAYER2_IDENTITY, TABLE_ID, PLAYER2_ADDRESS, '"Check"'):
            exit(1)
        phase = get_on_chain_phase()
    phase = get_on_chain_phase()
    print(f"  Phase after betting: {phase}")

# --- Generate two player keypairs (for MPC auth) ---

sk1 = SigningKey.generate()
sk2 = SigningKey.generate()
addr1 = encode_stellar_pubkey(bytes(sk1.verify_key))
addr2 = encode_stellar_pubkey(bytes(sk2.verify_key))

print(f"\nPlayer 1 (MPC): {addr1}")
print(f"Player 2 (MPC): {addr2}")

nonce = {addr1: 0, addr2: 0}

def next_nonce(addr):
    nonce[addr] += 1
    return nonce[addr]

# --- Card display helpers ---
SUITS = ["Spades", "Hearts", "Diamonds", "Clubs"]
RANKS = ["2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A"]

def decode_card(value: int) -> str:
    suit = SUITS[value // 13]
    rank = RANKS[value % 13]
    return f"{rank} of {suit}"

# --- Step 1: Health check ---
print("\n=== Health Check ===")
r = requests.get(f"{BASE}/api/health")
print(f"  {r.status_code}: {r.text}")

print("\n=== Committee Status ===")
r = requests.get(f"{BASE}/api/committee/status")
print(f"  {r.status_code}: {r.json()}")

if ON_CHAIN:
    phase = get_on_chain_phase()
    print(f"\n=== On-Chain Table Phase: {phase} ===")
    ensure_on_chain_ready_for_deal()
    phase = get_on_chain_phase()
    print(f"=== On-Chain Table Ready Phase: {phase} ===")
    initial_table_state = get_on_chain_table()
    initial_chip_total = table_chip_total(initial_table_state) if isinstance(initial_table_state, dict) else None
else:
    initial_chip_total = None

# --- Step 2: Request Deal ---
print("\n=== Request Deal (table {}, 2 players) ===".format(TABLE_ID))
headers = make_auth_headers(sk1, addr1, TABLE_ID, "request_deal", next_nonce(addr1))
payload = {"players": [addr1, addr2]}
r = requests.post(f"{BASE}/api/table/{TABLE_ID}/request-deal", json=payload, headers=headers, timeout=600)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    deal = r.json()
    print(f"  Deal response: {json.dumps(deal, indent=2)}")
else:
    print(f"  Body: {r.text[:2000]}")
    print(f"  Deal failed — stopping here.")
    exit(1)

# --- Step 2b: Retrieve Hole Cards ---
print("\n=== Retrieve Hole Cards: Player 1 ===")
headers = make_auth_headers(sk1, addr1, TABLE_ID, "get_player_cards", next_nonce(addr1))
r = requests.get(f"{BASE}/api/table/{TABLE_ID}/player/{addr1}/cards", headers=headers, timeout=30)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    cards1 = r.json()
    print(f"  Card 1: {cards1['card1']} ({decode_card(cards1['card1'])})")
    print(f"  Card 2: {cards1['card2']} ({decode_card(cards1['card2'])})")
    print(f"  Salt 1: {cards1['salt1']}")
    print(f"  Salt 2: {cards1['salt2']}")
else:
    print(f"  Error: {r.text}")
    print("  (hole card delivery not critical, continuing...)")

print("\n=== Retrieve Hole Cards: Player 2 ===")
headers = make_auth_headers(sk2, addr2, TABLE_ID, "get_player_cards", next_nonce(addr2))
r = requests.get(f"{BASE}/api/table/{TABLE_ID}/player/{addr2}/cards", headers=headers, timeout=30)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    cards2 = r.json()
    print(f"  Card 1: {cards2['card1']} ({decode_card(cards2['card1'])})")
    print(f"  Card 2: {cards2['card2']} ({decode_card(cards2['card2'])})")
    print(f"  Salt 1: {cards2['salt1']}")
    print(f"  Salt 2: {cards2['salt2']}")
else:
    print(f"  Error: {r.text}")
    print("  (hole card delivery not critical, continuing...)")

# --- On-chain: Preflop betting ---
if ON_CHAIN:
    do_preflop_betting()

# --- Step 3: Request Reveal Flop ---
print("\n=== Request Reveal: Flop ===")
headers = make_auth_headers(sk1, addr1, TABLE_ID, "request_reveal:flop", next_nonce(addr1))
r = requests.post(f"{BASE}/api/table/{TABLE_ID}/request-reveal/flop", headers=headers, timeout=600)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    flop = r.json()
    print(f"  Flop cards: {flop['cards']}")
    print(f"  Proof size: {flop['proof_size']}")
else:
    print(f"  Error: {r.text}")
    exit(1)

# --- On-chain: Flop betting ---
if ON_CHAIN:
    do_postflop_betting("Flop")

# --- Step 4: Request Reveal Turn ---
print("\n=== Request Reveal: Turn ===")
headers = make_auth_headers(sk1, addr1, TABLE_ID, "request_reveal:turn", next_nonce(addr1))
r = requests.post(f"{BASE}/api/table/{TABLE_ID}/request-reveal/turn", headers=headers, timeout=600)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    turn = r.json()
    print(f"  Turn card: {turn['cards']}")
    print(f"  Proof size: {turn['proof_size']}")
else:
    print(f"  Error: {r.text}")
    exit(1)

# --- On-chain: Turn betting ---
if ON_CHAIN:
    do_postflop_betting("Turn")

# --- Step 5: Request Reveal River ---
print("\n=== Request Reveal: River ===")
headers = make_auth_headers(sk1, addr1, TABLE_ID, "request_reveal:river", next_nonce(addr1))
r = requests.post(f"{BASE}/api/table/{TABLE_ID}/request-reveal/river", headers=headers, timeout=600)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    river = r.json()
    print(f"  River card: {river['cards']}")
    print(f"  Proof size: {river['proof_size']}")
else:
    print(f"  Error: {r.text}")
    exit(1)

# --- On-chain: River betting ---
if ON_CHAIN:
    do_postflop_betting("River")

# --- Step 6: Request Showdown ---
print("\n=== Request Showdown ===")
headers = make_auth_headers(sk1, addr1, TABLE_ID, "request_showdown", next_nonce(addr1))
r = requests.post(f"{BASE}/api/table/{TABLE_ID}/request-showdown", headers=headers, timeout=600)
print(f"  Status: {r.status_code}")
if r.status_code == 200:
    showdown = r.json()
    print(f"  Winner: {showdown['winner']}")
    print(f"  Winner index: {showdown['winner_index']}")
    print(f"  Proof size: {showdown['proof_size']}")
else:
    print(f"  Error: {r.text}")
    exit(1)

# --- Final on-chain check ---
if ON_CHAIN:
    phase = get_on_chain_phase()
    print(f"\n=== Final On-Chain Phase: {phase} ===")
    if phase != "Settlement":
        print("ERROR: flow finished but on-chain phase is not Settlement")
        exit(1)
    final_table_state = get_on_chain_table()
    if not isinstance(final_table_state, dict):
        print(f"ERROR: could not read final on-chain table state: {final_table_state}")
        exit(1)
    if int(final_table_state.get("pot", -1)) != 0:
        print(f"ERROR: expected settled pot to be 0, got {final_table_state.get('pot')}")
        exit(1)
    final_stack_total = player_stack_total(final_table_state)
    final_chip_total = table_chip_total(final_table_state)
    if initial_chip_total is not None and final_chip_total != initial_chip_total:
        print(f"ERROR: table chip total changed from {initial_chip_total} to {final_chip_total}")
        exit(1)
    print(f"  Settled pot: {final_table_state.get('pot')}")
    print(f"  Player stack total: {final_stack_total}")
    print(f"  Table chip total: {final_chip_total}")

print("\n=== FULL FLOW COMPLETE ===")
