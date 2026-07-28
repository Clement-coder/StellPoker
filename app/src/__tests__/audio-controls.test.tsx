import { render, fireEvent } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { AudioControls } from "../components/AudioControls";
import { getSfxMuted, getSfxVolume } from "../lib/sound-engine";

describe("AudioControls Component", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders mute toggle button and volume slider", () => {
    const { getByTestId } = render(<AudioControls />);
    expect(getByTestId("audio-mute-toggle")).toBeTruthy();
    expect(getByTestId("audio-volume-slider")).toBeTruthy();
  });

  it("toggles mute state and persists to localStorage", () => {
    const { getByTestId } = render(<AudioControls />);
    const btn = getByTestId("audio-mute-toggle");

    expect(btn.textContent).toContain("SFX");
    fireEvent.click(btn);
    expect(btn.textContent).toContain("MUTED");
    expect(localStorage.getItem("stellpoker-sfx-muted")).toBe("true");

    fireEvent.click(btn);
    expect(btn.textContent).toContain("SFX");
    expect(localStorage.getItem("stellpoker-sfx-muted")).toBe("false");
  });

  it("updates volume state and persists to localStorage", () => {
    const { getByTestId } = render(<AudioControls />);
    const slider = getByTestId("audio-volume-slider") as HTMLInputElement;

    fireEvent.change(slider, { target: { value: "0.8" } });
    expect(localStorage.getItem("stellpoker-sfx-volume")).toBe("0.8");
    expect(getSfxVolume()).toBe(0.8);
  });
});
