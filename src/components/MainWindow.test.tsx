// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
  configurable: true,
  value: vi.fn(),
});

const mocks = vi.hoisted(() => ({
  initSql: vi.fn().mockResolvedValue("SQL environment initialized"),
}));

vi.mock("../api", () => ({
  disconnect: vi.fn(),
  getObjectSource: vi.fn(),
  initSql: mocks.initSql,
  setUpdateMode: vi.fn(),
}));

vi.mock("./ObjectTree", () => ({ default: () => null }));
vi.mock("./Resizer", () => ({ default: () => null }));
vi.mock("./ShellTab", () => ({ default: () => null }));
vi.mock("./SqlTab", () => ({ default: () => null }));

import MainWindow from "./MainWindow";

let root: Root | undefined;
let host: HTMLDivElement | undefined;

afterEach(async () => {
  if (root) {
    await act(async () => root?.unmount());
  }
  host?.remove();
  root = undefined;
  host = undefined;
  mocks.initSql.mockClear();
});

describe("MainWindow SQL initialization", () => {
  it("initializes a session only once when React StrictMode re-runs effects", async () => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);

    await act(async () => {
      root?.render(
        <StrictMode>
          <MainWindow
            sessionId="sess-1"
            userHost="tester@localhost"
            dbType="MySQL"
            initialView="sql"
            initialShellHistory={[]}
            themePicker={null}
            onDbTypeDetected={vi.fn()}
            onDisconnected={vi.fn()}
          />
        </StrictMode>,
      );
      await Promise.resolve();
    });

    expect(mocks.initSql).toHaveBeenCalledTimes(1);
    expect(mocks.initSql).toHaveBeenCalledWith("sess-1", "MySQL");
  });
});
