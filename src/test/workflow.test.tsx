// Primary workflow: Import → Scan → Review → Organise → Verify results.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createBackend } from "./fakeBackend";

const backend = vi.hoisted(() => ({ current: null as ReturnType<typeof import("./fakeBackend").createBackend> | null }));
const pickQueue = vi.hoisted(() => [] as string[][]);

vi.mock("../services/api", () => ({
  api: new Proxy({}, { get: (_t, k: string) => (backend.current!.api as Record<string, unknown>)[k] }),
  events: new Proxy({}, { get: (_t, k: string) => (backend.current!.events as Record<string, unknown>)[k] }),
  errorMessage: (e: unknown) => String(e instanceof Error ? e.message : e),
}));
vi.mock("../services/dialogs", () => ({
  pickFolders: vi.fn(async () => pickQueue.shift() ?? []),
  pickJsonFile: vi.fn(async () => null),
  pickSavePath: vi.fn(async () => null),
  confirmDialog: vi.fn(async () => true),
}));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `kitaudio://localhost/${encodeURIComponent(p)}`, invoke: vi.fn() }));

import App from "../App";

describe("primary workflow", () => {
  beforeEach(() => {
    backend.current = createBackend();
    pickQueue.length = 0;
  });

  it("imports, scans, reviews, organises and shows results", async () => {
    const user = userEvent.setup();
    render(<App />);

    // Import
    await user.click(within(await screen.findByRole("navigation", { name: "Main" })).getByRole("button", { name: /import samples/i }));
    pickQueue.push(["/packs/XLNT"]);
    await user.click(screen.getByRole("button", { name: /browse folders/i }));
    const queue = await screen.findByRole("list", { name: /import queue/i });
    expect(within(queue).getByText("XLNT")).toBeInTheDocument();
    pickQueue.push(["/out"]);
    await user.click(screen.getByRole("button", { name: /^choose$/i }));
    expect(await screen.findByText("/out")).toBeInTheDocument();

    // Scan
    await user.click(screen.getByRole("button", { name: /scan 1 folder/i }));
    expect(backend.current!.calls.find((c) => c.name === "startScan")?.args).toEqual([["/packs/XLNT"], "/out", "copy"]);

    // Review (navigated automatically when the scan finishes)
    const table = await screen.findByRole("table", { name: /samples/i });
    await within(table).findByText("XLNT_Growl_F.wav");
    expect(within(table).getByText("Bass/Growls")).toBeInTheDocument();
    expect(within(table).getByText("Needs review")).toBeInTheDocument();

    // Filter to uncertain samples, inspect and reassign one
    await user.click(screen.getByRole("tab", { name: /needs review/i }));
    // Needs review opens grouped; switch to the file list.
    await screen.findByRole("list", { name: /review groups/i });
    await user.click(screen.getByRole("radio", { name: /files/i }));
    const reviewTable = await screen.findByRole("table", { name: /samples/i });
    await waitFor(() => expect(within(reviewTable).queryByText("Kick_01.wav")).not.toBeInTheDocument());
    await user.click(within(reviewTable).getByText("Mystery_01.wav"));
    const details = screen.getByRole("complementary", { name: /sample details/i });
    expect(within(details).getByText(/Evidence/)).toBeInTheDocument();
    await user.click(within(details).getByRole("button", { name: /change category/i }));
    await user.click(within(details).getByRole("option", { name: "Kicks" }));
    expect(backend.current!.calls.find((c) => c.name === "reassignSamples")?.args).toEqual([["s3"], "kicks"]);
    await user.click(screen.getByRole("tab", { name: /^all/i }));

    // Organise: plan preview first, nothing runs until confirmed
    await user.click(screen.getByRole("button", { name: /organise samples/i }));
    const dialog = await screen.findByRole("dialog");
    expect(await within(dialog).findByText("Files to copy")).toBeInTheDocument();
    expect(backend.current!.calls.some((c) => c.name === "organise")).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: /copy 3 files/i }));

    // Results
    expect(await screen.findByText("Your samples are organised")).toBeInTheDocument();
    expect(backend.current!.calls.find((c) => c.name === "organise")?.args).toEqual(["imp1", "/out", "copy", false]);
    expect(backend.current!.samples().every((s) => s.status === "organised")).toBe(true);

    // Verify in the library
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /open library/i }));
    const libTable = await screen.findByRole("table", { name: /samples/i });
    expect(await within(libTable).findByText("Kick_01.wav")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: /library folders/i })).toHaveTextContent("Growls");
  });

  it("requires explicit confirmation for move mode", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(within(await screen.findByRole("navigation", { name: "Main" })).getByRole("button", { name: /import samples/i }));
    pickQueue.push(["/packs/XLNT"], ["/out"]);
    await user.click(screen.getByRole("button", { name: /browse folders/i }));
    await user.click(screen.getByRole("button", { name: /^choose$/i }));
    await user.click(screen.getByRole("radio", { name: /move/i }));
    expect(screen.getByText(/move mode removes files/i)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /scan 1 folder/i }));
    await screen.findByRole("table", { name: /samples/i });

    await user.click(screen.getByRole("button", { name: /organise samples/i }));
    const dialog = await screen.findByRole("dialog");
    const go = await within(dialog).findByRole("button", { name: /move 3 files/i });
    expect(go).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox"));
    expect(go).toBeEnabled();
    await user.click(go);
    await screen.findByText("Your samples are organised");
    expect(backend.current!.calls.find((c) => c.name === "organise")?.args).toEqual(["imp1", "/out", "move", true]);
  });

  it("offers to apply a correction to similar files", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(within(await screen.findByRole("navigation", { name: "Main" })).getByRole("button", { name: /import samples/i }));
    pickQueue.push(["/packs/XLNT"], ["/out"]);
    await user.click(screen.getByRole("button", { name: /browse folders/i }));
    await user.click(screen.getByRole("button", { name: /^choose$/i }));
    await user.click(screen.getByRole("button", { name: /scan 1 folder/i }));
    const table = await screen.findByRole("table", { name: /samples/i });

    backend.current!.api.similarSamples = async () => ({ ids: ["s1", "s2"], description: "files named like “mystery”", categoryId: "kicks", categoryPath: "Kicks" });
    await user.click(await within(table).findByText("Mystery_01.wav"));
    const details = screen.getByRole("complementary", { name: /sample details/i });
    await user.click(within(details).getByRole("button", { name: /change category/i }));
    await user.click(within(details).getByRole("option", { name: "Kicks" }));

    const banner = await screen.findByRole("status", { name: /similar files/i });
    expect(banner).toHaveTextContent("Found 2 more files named like “mystery”");
    await user.click(within(banner).getByRole("button", { name: /apply to 2/i }));
    const calls = backend.current!.calls.filter((c) => c.name === "reassignSamples");
    expect(calls.at(-1)?.args).toEqual([["s1", "s2"], "kicks"]);
    await waitFor(() => expect(screen.queryByRole("status", { name: /similar files/i })).not.toBeInTheDocument());
  });

  it("reviews in groups and applies decisions to whole groups", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(within(await screen.findByRole("navigation", { name: "Main" })).getByRole("button", { name: /import samples/i }));
    pickQueue.push(["/packs/XLNT"], ["/out"]);
    await user.click(screen.getByRole("button", { name: /browse folders/i }));
    await user.click(screen.getByRole("button", { name: /^choose$/i }));
    await user.click(screen.getByRole("button", { name: /scan 1 folder/i }));
    await screen.findByRole("table", { name: /samples/i });

    await user.click(screen.getByRole("tab", { name: /needs review/i }));
    const groups = await screen.findByRole("list", { name: /review groups/i });
    expect(screen.getByText("No recognisable keywords")).toBeInTheDocument();
    const row = within(groups).getByText("XLNT/Drums").closest("li")!;
    await user.click(within(row as HTMLElement).getByRole("button", { name: /^accept$/i }));
    const call = backend.current!.calls.find((c) => c.name === "acceptMatching");
    expect(call?.args[0]).toMatchObject({ importId: "imp1", relativeDir: "XLNT/Drums", categoryId: "snares" });
    expect(await screen.findByText(/Accepted 12 file\(s\)\. 30 similar file\(s\) were resolved automatically\. 5 left to review\./)).toBeInTheDocument();

    await user.click(within(row as HTMLElement).getByRole("button", { name: /files/i }));
    expect(await screen.findByRole("button", { name: /clear folder filter/i })).toBeInTheDocument();
  });
});
