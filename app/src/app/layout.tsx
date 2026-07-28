import type { Metadata, Viewport } from "next";
import "./globals.css";
import { I18nProvider } from "@/lib/i18n/context";
import { ThemeToggle } from "@/components/ThemeToggle";

export const metadata: Metadata = {
  title: "Poker on Stellar",
  description: "Onchain poker with private cards via MPC + ZK proofs on Stellar",
};

// Ensure the viewport is correctly sized on mobile browsers and that
// Freighter's in-app browser (Safari/Chrome WebView) doesn't auto-zoom
// input fields by keeping font-size at 16px minimum (#18).
export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  maximumScale: 1,
  userScalable: false,
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
            <ThemeToggle />
          </div>
          {children}
        </I18nProvider>
      </body>
    </html>
  );
}
