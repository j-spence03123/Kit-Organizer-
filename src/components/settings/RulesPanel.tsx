import type { CategoryConfig, ClassifierWeights } from "../../types";
import { Field, inputClass } from "../ui";

const WEIGHTS: [keyof ClassifierWeights, string, string][] = [
  ["filename", "Filename match", "Points for a keyword in the filename"],
  ["phraseBonus", "Phrase bonus", "Extra points per additional word in a phrase"],
  ["folderNear", "Parent folder match", "Points for a keyword in the immediate folder"],
  ["folderDecay", "Folder decay", "Multiplier per extra folder level"],
  ["rootFolderFactor", "Imported folder factor", "Multiplier for the top-level pack folder"],
  ["parentFactor", "Parent category factor", "Multiplier for broad categories (e.g. Bass)"],
  ["weakFactor", "Weak alias factor", "Multiplier for ~weak aliases"],
  ["ancestorSupport", "Ancestor support", "How much a parent match supports its children"],
  ["headNounBonus", "Head-noun bonus", "Bonus for the last keyword when names conflict"],
  ["priorityStep", "Priority step", "Points per category priority level"],
  ["saturation", "Saturation", "Higher = more evidence needed for high confidence"],
  ["loopThreshold", "Loop threshold", "Loop score needed to treat a sample as a loop"],
  ["minScore", "Minimum score", "Below this the file goes to Uncategorised"],
  ["metadata", "Embedded tag match", "Points for a keyword in title/genre/comment tags"],
  ["fuzzyFactor", "Typo match factor", "Multiplier for misspelt keywords"],
  ["learned", "Learned rule", "Points for matching one of your corrections"],
  ["sibling", "Sibling agreement", "Points when other files in the folder agree"],
  ["audio", "Audio analysis factor", "Multiplier for audio evidence (0 = off)"],
];

function terms(list: string[]): string {
  return list.join(", ");
}
function parse(s: string): string[] {
  return s
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);
}

export function RulesPanel({ draft, onChange }: { draft: CategoryConfig; onChange: (c: CategoryConfig) => void }) {
  return (
    <div className="grid gap-5">
      <Field label="Library folder name" hint="Created inside the output folder you choose when importing.">
        <input className={inputClass} value={draft.rootFolder} onChange={(e) => onChange({ ...draft, rootFolder: e.target.value })} aria-label="Library folder name" />
      </Field>
      <div>
        <h3 className="mb-2 text-xs font-medium text-muted">Scoring weights</h3>
        <div className="grid grid-cols-2 gap-3">
          {WEIGHTS.map(([k, label, hint]) => (
            <label key={k} className="grid gap-1" title={hint}>
              <span className="text-[11px] text-faint">{label}</span>
              <input
                type="number"
                step="0.05"
                className={inputClass}
                value={draft.weights[k]}
                onChange={(e) => onChange({ ...draft, weights: { ...draft.weights, [k]: Number(e.target.value) } })}
              />
            </label>
          ))}
        </div>
      </div>
      <Field label="Generic folder names" hint="Folders with these names add no category evidence (e.g. Samples, One Shots).">
        <textarea className={`${inputClass} h-20 py-1.5`} defaultValue={terms(draft.genericFolderTerms)} onBlur={(e) => onChange({ ...draft, genericFolderTerms: parse(e.target.value) })} aria-label="Generic folder names" />
      </Field>
      <Field label="Vendor prefixes" hint="Leading words like XLNT_ or BOS_ that are ignored in filenames. Unknown ALL-CAPS prefixes are detected automatically.">
        <textarea className={`${inputClass} h-16 py-1.5`} defaultValue={terms(draft.vendorPrefixes ?? [])} onBlur={(e) => onChange({ ...draft, vendorPrefixes: parse(e.target.value) })} aria-label="Vendor prefixes" />
      </Field>
      <Field label="Loop indicators">
        <textarea className={`${inputClass} h-16 py-1.5`} defaultValue={terms(draft.loopTerms)} onBlur={(e) => onChange({ ...draft, loopTerms: parse(e.target.value) })} aria-label="Loop indicators" />
      </Field>
      <Field label="One-shot indicators">
        <textarea className={`${inputClass} h-16 py-1.5`} defaultValue={terms(draft.oneShotTerms)} onBlur={(e) => onChange({ ...draft, oneShotTerms: parse(e.target.value) })} aria-label="One-shot indicators" />
      </Field>
    </div>
  );
}
