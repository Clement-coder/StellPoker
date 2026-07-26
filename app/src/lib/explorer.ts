const DEFAULT_STELLAR_EXPERT_BASE_URL = "https://stellar.expert/explorer/testnet";

function normalizeBaseUrl(url: string | undefined): string {
  const value = (url || DEFAULT_STELLAR_EXPERT_BASE_URL).trim();
  return value.replace(/\/+$/, "");
}

export function stellarExpertUrl(
  resource: "tx" | "account" | "contract",
  id: string
): string {
  return `${normalizeBaseUrl(process.env.NEXT_PUBLIC_STELLAR_EXPERT_BASE_URL)}/${resource}/${id}`;
}
