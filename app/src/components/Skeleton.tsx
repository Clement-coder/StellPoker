"use client";

export function Skeleton({ className = "", width = "100%", height = "16px" }: { className?: string; width?: string; height?: string }) {
  return (
    <div
      className={`rounded-sm animate-pulse bg-[rgba(255,255,255,0.06)] ${className}`}
      style={{ width, height }}
      aria-hidden
    />
  );
}
