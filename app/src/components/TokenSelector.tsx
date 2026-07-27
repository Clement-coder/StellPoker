"use client";

import { useState } from "react";

export type TokenChoice = {
  type: "XLM" | "SAC";
  sacAddress?: string;
};

export function TokenSelector({ value, onChange }: { value?: TokenChoice; onChange: (v: TokenChoice) => void }) {
  const [choice, setChoice] = useState<TokenChoice>(value ?? { type: "XLM" });

  const update = (next: TokenChoice) => {
    setChoice(next);
    onChange(next);
  };

  return (
    <div className="flex items-center gap-2">
      <label className="text-[9px]">Token:</label>
      <select
        value={choice.type}
        onChange={(e) => update({ type: e.target.value as any })}
        className="text-[9px]"
        style={{ padding: '6px', border: '2px solid var(--ui-border)', background: 'transparent' }}
      >
        <option value="XLM">XLM (native)</option>
        <option value="SAC">Custom SAC</option>
      </select>
      {choice.type === "SAC" && (
        <input
          placeholder="Enter SAC contract address"
          value={choice.sacAddress ?? ""}
          onChange={(e) => update({ ...choice, sacAddress: e.target.value })}
          className="text-[9px]"
          style={{ padding: '6px', border: '2px solid var(--ui-border)', background: 'transparent' }}
        />
      )}
    </div>
  );
}
