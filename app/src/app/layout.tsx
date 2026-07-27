import type { Metadata } from "next";
import "./globals.css";
import { I18nProvider } from "@/lib/i18n/context";
import dynamic from "next/dynamic";

const ThemeToggle = dynamic(() => import("@/components/ThemeToggle").then(m => m.ThemeToggle), { ssr: false });

export const metadata: Metadata = {
  title: "Poker on Stellar",
  description: "Onchain poker with private cards via MPC + ZK proofs on Stellar",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>
        <I18nProvider>
          <div style={{ position: "absolute", top: 8, right: 8, zIndex: 9999 }}>
            {/* ThemeToggle is client-side only */}
            <ThemeToggle />
          </div>
          {children}
        </I18nProvider>
      </body>
    </html>
  );
}
