import { render } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Skeleton } from "../components/Skeleton";

describe("Skeleton", () => {
  it("renders with provided width and height", () => {
    const { container } = render(<Skeleton width="120px" height="12px" /> as any);
    const div = container.querySelector("div");
    expect(div).toBeTruthy();
    expect(div?.getAttribute("style") || "").toContain("width: 120px");
  });
});
