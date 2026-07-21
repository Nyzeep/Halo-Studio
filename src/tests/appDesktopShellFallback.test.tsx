import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "../renderer/App";

describe("App desktop shell fallback", () => {
  it("shows a clear desktop-shell message instead of crashing when Halo API is unavailable", () => {
    delete (window as Partial<Window>).halo;

    render(<App />);

    expect(screen.getByText("需要通过 Halo Studio 桌面壳启动")).toBeInTheDocument();
    expect(screen.getByText("npm run dev:electron")).toBeInTheDocument();
  });
});
