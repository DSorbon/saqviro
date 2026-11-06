// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App.svelte";
import { tool } from "../lib/fixtures";
import type { JobSnapshot } from "../lib/job";

const pdf = { inputs: [{ type: "pdf", extensions: ["pdf"] }] };
const tools = [
  tool("pdf.compress", "Compress PDF", pdf),
  tool("pdf.merge", "Merge PDF", { ...pdf, capabilities: { batch: false } }),
  tool("image.compress", "Compress Images", { inputs: [{ type: "image", extensions: ["png"] }] }),
  tool("files.zip.create", "Create ZIP", { inputs: [{ type: "any", extensions: ["*"] }] }),
];

const api = vi.hoisted(() => ({
  drop: null as ((paths: string[]) => void) | null,
  jobListeners: [] as ((job: JobSnapshot) => void)[],
  toolsForFiles: vi.fn(),
  adviseFiles: vi.fn(),
  runTool: vi.fn(),
  updatesEnabled: vi.fn(),
  checkUpdate: vi.fn(),
  installUpdate: vi.fn(),
  openLicenses: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  listTools: async () => tools,
  toolsForFiles: api.toolsForFiles,
  adviseFiles: api.adviseFiles,
  runTool: api.runTool,
  pickFiles: async () => [],
  cancelJob: async () => true,
  revealFile: async () => undefined,
  openFile: async () => undefined,
  saveImage: async () => true,
  appVersion: async () => "1.2.3",
  updatesEnabled: api.updatesEnabled,
  checkUpdate: api.checkUpdate,
  installUpdate: api.installUpdate,
  openLicenses: api.openLicenses,
  onFileDrop: (drop: (paths: string[]) => void) => {
    api.drop = drop;
    return Promise.resolve(() => undefined);
  },
  onJobEvent: (cb: (job: JobSnapshot) => void) => {
    api.jobListeners.push(cb);
    return Promise.resolve(() => undefined);
  },
}));

const nav = () => within(screen.getByRole("navigation"));

beforeEach(() => {
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  api.jobListeners.length = 0;
  api.toolsForFiles.mockReset().mockResolvedValue(["pdf.compress", "pdf.merge", "files.zip.create"]);
  api.adviseFiles.mockReset().mockResolvedValue([]);
  api.runTool.mockReset().mockResolvedValue(7);
  api.updatesEnabled.mockReset().mockResolvedValue(true);
  api.checkUpdate.mockReset().mockResolvedValue(null);
  api.installUpdate.mockReset().mockResolvedValue(undefined);
  api.openLicenses.mockReset().mockResolvedValue(undefined);
});

describe("home", () => {
  it("offers popular tools to someone new, not the first in the alphabet", async () => {
    render(App);
    expect(await screen.findByText("Popular")).toBeTruthy();
    const names = screen.getAllByRole("button").map((b) => b.textContent);
    expect(names.findIndex((n) => n?.startsWith("Compress PDF"))).toBeLessThan(
      names.findIndex((n) => n?.startsWith("Compress Images")),
    );
  });

  it("keeps favorites across restarts", async () => {
    const first = render(App);
    await fireEvent.click(await screen.findByRole("button", { name: /Create ZIP/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Favorite" }));
    expect(nav().getByRole("button", { name: "Favorites" })).toBeTruthy();
    first.unmount();

    render(App);
    expect(await screen.findByText("Favorites & recent")).toBeTruthy();
    await fireEvent.click(nav().getByRole("button", { name: "Favorites" }));
    expect(screen.getByRole("button", { name: /Create ZIP/ })).toBeTruthy();
  });

  it("records a tool only when it runs", async () => {
    render(App);
    await fireEvent.click(await screen.findByRole("button", { name: /Create ZIP/ }));
    expect(nav().queryByRole("button", { name: "Recent" })).toBeNull();

    await api.drop!(["/docs/a.pdf"]);
    await vi.waitFor(() => expect(screen.getByRole("button", { name: /Run/ }).hasAttribute("disabled")).toBe(false));
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    expect(await nav().findByRole("button", { name: "Recent" })).toBeTruthy();
  });
});

describe("dropping files", () => {
  it("lists tools for the file first and narrows the sidebar", async () => {
    render(App);
    await screen.findByText("Popular");
    expect(nav().getByRole("button", { name: "Images" })).toBeTruthy();

    await api.drop!(["/docs/a.pdf"]);

    expect(await screen.findByText("For any file")).toBeTruthy();
    expect(nav().queryByRole("button", { name: "Images" })).toBeNull();
    expect(nav().getByRole("button", { name: "PDF" })).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(nav().getByRole("button", { name: "Images" })).toBeTruthy();
  });

  it("sends results on to the next tool", async () => {
    render(App);
    await api.drop!(["/docs/a.pdf"]);
    await fireEvent.click(await screen.findByRole("button", { name: /Compress PDF/ }));
    await fireEvent.click(screen.getByRole("button", { name: /Run/ }));
    await vi.waitFor(() => expect(api.runTool).toHaveBeenCalled());
    const out = { path: "/docs/a-compressed.pdf", size: 1, source_size: 2 };
    api.jobListeners.forEach((l) =>
      l({
        id: 7,
        tool_id: "pdf.compress",
        status: "completed",
        progress: 1,
        output: { kind: "files", value: [out] },
        error: null,
        started_at: 1,
        finished_at: 2,
      }),
    );

    await fireEvent.click(await screen.findByRole("button", { name: "Use in another tool…" }));

    expect(await screen.findByRole("heading", { name: "a-compressed.pdf" })).toBeTruthy();
    expect(api.toolsForFiles).toHaveBeenLastCalledWith([out.path]);
  });
});

describe("search", () => {
  it("moves through results with the keyboard and opens the selected one", async () => {
    render(App);
    await screen.findByText("Popular");
    const search = screen.getByLabelText("Search tools");
    await fireEvent.input(search, { target: { value: "compress" } });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(document.querySelector('[aria-current="true"]')?.textContent).toContain("Compress PDF");

    await fireEvent.keyDown(search, { key: "Enter" });

    expect(screen.getByRole("heading", { name: "Compress PDF" })).toBeTruthy();
  });

  it("puts a tool you use ahead of its equals", async () => {
    localStorage.setItem(
      "saqviro.prefs",
      JSON.stringify({ usage: { "pdf.compress": { count: 9, last: Date.now() } } }),
    );
    render(App);
    await screen.findByText("Favorites & recent");
    await fireEvent.input(screen.getByLabelText("Search tools"), { target: { value: "compress" } });
    expect(document.querySelector('[aria-current="true"]')?.textContent).toContain("Compress PDF");
  });
});

describe("settings", () => {
  it("applies and remembers the theme", async () => {
    const first = render(App);
    await fireEvent.click(await nav().findByRole("button", { name: "Settings" }));
    await fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    first.unmount();

    render(App);
    await fireEvent.click(await nav().findByRole("button", { name: "Settings" }));
    expect(screen.getByRole("radio", { name: "Dark" }).getAttribute("aria-checked")).toBe("true");

    await fireEvent.click(screen.getByRole("radio", { name: "System" }));
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });

  it("clears history but keeps favorites", async () => {
    localStorage.setItem(
      "saqviro.prefs",
      JSON.stringify({ favorites: ["pdf.merge"], usage: { "pdf.compress": { count: 1, last: 1 } } }),
    );
    render(App);
    await fireEvent.click(await nav().findByRole("button", { name: "Settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Clear recent tools" }));
    expect(nav().queryByRole("button", { name: "Recent" })).toBeNull();
    expect(nav().getByRole("button", { name: "Favorites" })).toBeTruthy();
  });

  it("opens with the keyboard", async () => {
    render(App);
    await screen.findByText("Popular");
    await fireEvent.keyDown(window, { key: ",", metaKey: true });
    expect(screen.getByRole("heading", { name: "Settings" })).toBeTruthy();
  });
});

describe("updates", () => {
  const openSettings = async () => {
    render(App);
    await fireEvent.click(await nav().findByRole("button", { name: "Settings" }));
  };

  it("never checks on its own", async () => {
    await openSettings();
    await screen.findByText("Saqviro 1.2.3");
    expect(api.checkUpdate).not.toHaveBeenCalled();
  });

  it("offers nothing in a build that cannot verify updates", async () => {
    api.updatesEnabled.mockResolvedValue(false);
    await openSettings();
    await screen.findByText("Saqviro 1.2.3");
    expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
  });

  it("says when the app is up to date", async () => {
    await openSettings();
    await fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Saqviro is up to date.")).toBeTruthy();
  });

  it("installs only when asked", async () => {
    api.checkUpdate.mockResolvedValue({ version: "2.0.0", notes: null });
    await openSettings();
    await fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Version 2.0.0 is available.")).toBeTruthy();
    expect(api.installUpdate).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Install and restart" }));
    expect(api.installUpdate).toHaveBeenCalledOnce();
  });

  it("shows a failed check", async () => {
    api.checkUpdate.mockRejectedValue({ kind: "unknown", message: "could not check for updates: offline" });
    await openSettings();
    await fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect((await screen.findByRole("alert")).textContent).toContain("offline");
  });
});

describe("Smart Drop advice", () => {
  it("recommends a tool and says why", async () => {
    api.adviseFiles.mockResolvedValue([{ tool: "pdf.compress", reason: "This PDF is 80.0 MB" }]);
    render(App);
    await screen.findByText("Compress PDF");
    api.drop!(["/big.pdf"]);
    const list = await screen.findByRole("list", { name: "Recommended" });
    expect(within(list).getByText("This PDF is 80.0 MB")).toBeTruthy();
    await fireEvent.click(within(list).getByRole("button"));
    expect(await screen.findByRole("heading", { name: "Compress PDF" })).toBeTruthy();
  });

  it("leaves out tools that cannot take these files", async () => {
    api.adviseFiles.mockResolvedValue([{ tool: "image.compress", reason: "Large" }]);
    render(App);
    await screen.findByText("Compress PDF");
    api.drop!(["/a.pdf"]);
    await screen.findByText("a.pdf");
    await Promise.resolve();
    expect(screen.queryByRole("list", { name: "Recommended" })).toBeNull();
  });

  it("works without advice", async () => {
    api.adviseFiles.mockRejectedValue(new Error("no"));
    render(App);
    await screen.findByText("Compress PDF");
    api.drop!(["/a.pdf"]);
    expect(await screen.findByText("a.pdf")).toBeTruthy();
  });
});
