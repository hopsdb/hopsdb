// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

vi.mock("../api", () => ({
  connect: vi.fn(),
  isTauriRuntime: true,
}));

import ConnectionManager from "./ConnectionManager";

let root: Root | undefined;
let host: HTMLDivElement | undefined;

afterEach(async () => {
  if (root) await act(async () => root?.unmount());
  host?.remove();
  root = undefined;
  host = undefined;
  localStorage.clear();
});

describe("ConnectionManager SSH form", () => {
  it("explains that automatic SQL view detection requires a default client prompt", async () => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);

    await act(async () => {
      root?.render(<ConnectionManager onConnected={vi.fn()} />);
    });
    await act(async () => {
      host?.querySelector<HTMLElement>(".btn-small")?.click();
    });

    expect(host.textContent).toContain("Do not customize the client prompt");
    expect(host.textContent).toContain("NEWSQL>");
  });

  it("migrates the old first SSH command into the structured SSH fields", async () => {
    localStorage.setItem(
      "hopsdb.connections",
      JSON.stringify([
        {
          id: "legacy",
          label: "Legacy Oracle",
          commands: "ssh -p 2222 oracle@127.0.0.1\nsqlplus system/password@localhost:1521/FREEPDB1",
          commandPasswords: [{ lineIndex: 0, password: "ssh-password" }],
        },
      ]),
    );
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);

    await act(async () => {
      root?.render(<ConnectionManager onConnected={vi.fn()} />);
    });
    await act(async () => {
      host?.querySelector<HTMLElement>(".conn-item")?.click();
    });

    expect(host.querySelector<HTMLInputElement>('input[autocomplete="username"]')?.value).toBe("oracle");
    expect(host.querySelector<HTMLInputElement>('input[placeholder="host or IP"]')?.value).toBe("127.0.0.1");
    expect(host.querySelector<HTMLInputElement>('input[type="number"]')?.value).toBe("2222");
    expect(host.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe(
      "sqlplus system/password@localhost:1521/FREEPDB1",
    );
  });

  it("migrates the oldest user and host profile without treating its command set as SSH", async () => {
    localStorage.setItem(
      "hopsdb.connections",
      JSON.stringify([
        {
          id: "oldest",
          label: "Oldest profile",
          user: "oracle",
          host: "127.0.0.1:2222",
          password: "ssh-password",
          commands: "sqlplus system/password@localhost:1521/FREEPDB1",
        },
      ]),
    );
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);

    await act(async () => {
      root?.render(<ConnectionManager onConnected={vi.fn()} />);
    });
    await act(async () => {
      host?.querySelector<HTMLElement>(".conn-item")?.click();
    });

    expect(host.querySelector<HTMLInputElement>('input[autocomplete="username"]')?.value).toBe("oracle");
    expect(host.querySelector<HTMLInputElement>('input[placeholder="host or IP"]')?.value).toBe("127.0.0.1");
    expect(host.querySelector<HTMLInputElement>('input[type="number"]')?.value).toBe("2222");
    expect(host.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe(
      "sqlplus system/password@localhost:1521/FREEPDB1",
    );
  });
});
