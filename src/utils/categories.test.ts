import { describe, expect, it } from "vitest";
import { config } from "../test/fakeBackend";
import { buildTree } from "../components/library/LibraryTree";
import { aliasConflicts, cloneConfig, descendantIds, findNode, flatten, redirectReferences, referencesTo, validateName } from "./categories";
import { formatBytes, formatDuration, joinPath } from "./format";

describe("category helpers", () => {
  it("flattens with paths and depth", () => {
    const flat = flatten(config.categories);
    expect(flat.find((f) => f.id === "bass/growls")).toMatchObject({ path: "Bass/Growls", depth: 1, parentId: "bass" });
  });

  it("validates folder-safe, unique names", () => {
    const siblings = config.categories;
    expect(validateName("Kicks", siblings)).toMatch(/already exists/);
    expect(validateName("Kicks", siblings, "kicks")).toBeNull();
    expect(validateName("A/B", siblings)).toMatch(/cannot contain/);
    expect(validateName("", siblings)).toMatch(/empty/);
    expect(validateName("Trailing.", siblings)).toMatch(/period/);
  });

  it("finds references and redirects them", () => {
    const c = cloneConfig(config);
    findNode(c.categories, "kicks")!.loopTarget = "bass/growls";
    expect(referencesTo(c, ["bass/growls"]).map((r) => r.id)).toEqual(["kicks"]);
    redirectReferences(c.categories, new Set(["bass/growls"]), "snares");
    expect(findNode(c.categories, "kicks")!.loopTarget).toBe("snares");
    expect(findNode(config.categories, "kicks")!.loopTarget).toBeUndefined();
  });

  it("detects alias conflicts ignoring weak marker and punctuation", () => {
    const c = cloneConfig(config);
    findNode(c.categories, "snares")!.aliases.push("~Kicks");
    expect(aliasConflicts(c).get("kicks")).toEqual(["Kicks", "Snares"]);
  });

  it("collects descendants", () => {
    expect(descendantIds(findNode(config.categories, "bass")!)).toEqual(["bass", "bass/growls", "bass/neuros"]);
  });
});

describe("library tree", () => {
  it("nests paths and sums counts", () => {
    const t = buildTree([["Bass/Growls", 3], ["Bass/Neuros", 2], ["Kicks", 5]]);
    expect(t.map((n) => [n.name, n.count])).toEqual([["Bass", 5], ["Kicks", 5]]);
    expect(t[0]!.children.map((c) => c.path)).toEqual(["Bass/Growls", "Bass/Neuros"]);
  });
});

describe("formatting", () => {
  it("formats values and never invents data", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatDuration(null)).toBe("Unknown");
    expect(formatDuration(1.234)).toBe("1.23s");
    expect(joinPath("/out/", "Organised Samples", "Kicks")).toBe("/out/Organised Samples/Kicks");
  });
});
