// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

const mocks = vi.hoisted(() => ({
  runSql: vi.fn().mockResolvedValue({ columns: ["ID", "NAME"], rows: [["1", "Ada"]], text: [] }),
}));

vi.mock("@uiw/react-codemirror", () => ({ default: () => <textarea /> }));
vi.mock("../api", () => ({
  explainPlan: vi.fn(),
  exportCsv: vi.fn(),
  exportExcel: vi.fn(),
  formatSql: vi.fn((value: string) => value),
  getColumnList: vi.fn().mockResolvedValue([]),
  getTableList: vi.fn().mockResolvedValue([]),
  runSql: mocks.runSql,
}));

import SqlTab from "./SqlTab";

let root: Root | undefined;
let host: HTMLDivElement | undefined;

afterEach(async () => {
  if (root) await act(async () => root?.unmount());
  host?.remove();
  root = undefined;
  host = undefined;
  mocks.runSql.mockClear();
});

describe("SQL result table", () => {
  it("renders a drag handle for each result column", async () => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    let actions: { run: () => void } | null = null;

    await act(async () => {
      root?.render(
        <SqlTab
          sessionId="session-1"
          initialContent="select 1"
          updateMode={false}
          dbType="PostgreSQL"
          onRegisterActions={(next) => { actions = next; }}
        />,
      );
      await Promise.resolve();
    });

    await act(async () => {
      actions?.run();
      await Promise.resolve();
    });

    expect(host.querySelectorAll('[data-testid^="column-resizer-"]')).toHaveLength(2);
  });

  it("updates the column width when its handle is dragged", async () => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    let actions: { run: () => void } | null = null;

    await act(async () => {
      root?.render(
        <SqlTab
          sessionId="session-1"
          initialContent="select 1"
          updateMode={false}
          dbType="PostgreSQL"
          onRegisterActions={(next) => { actions = next; }}
        />,
      );
      await Promise.resolve();
    });
    await act(async () => {
      actions?.run();
      await Promise.resolve();
    });

    const handle = host.querySelector<HTMLElement>('[data-testid="column-resizer-c0"]');
    const header = host.querySelector<HTMLElement>("th");
    expect(handle).not.toBeNull();
    expect(header?.style.width).toBe("180px");

    await act(async () => {
      handle?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, clientX: 180 }));
      document.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: 240 }));
      document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, clientX: 240 }));
    });

    expect(header?.style.width).toBe("240px");
  });
});
