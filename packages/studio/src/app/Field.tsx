import type { PropSchema } from "@karkataka/runtime";
import { createSignal, For, Index, Match, Show, Switch } from "solid-js";
import { kindOf, toNumber } from "../fields.ts";

/** An editor for one value, chosen from its schema (or its value). `undefined` means "remove this field". */
export function Field(p: { name: string; schema?: PropSchema; value: unknown; onChange: (value: unknown) => void }) {
  const kind = () => kindOf(p.schema, p.value);
  const id = () => `field-${p.name.replace(/\W/g, "-")}`;
  const [jsonError, setJsonError] = createSignal(false);
  return (
    <div class="field">
      <label for={id()}>{p.schema?.title ?? p.name}</label>
      <Switch>
        <Match when={kind() === "text"}>
          <input id={id()} value={String(p.value ?? "")} onInput={(e) => p.onChange(e.currentTarget.value === "" ? undefined : e.currentTarget.value)} />
        </Match>
        <Match when={kind() === "multiline"}>
          <textarea id={id()} rows={Math.min(12, String(p.value ?? "").split("\n").length + 1)} value={String(p.value ?? "")} onInput={(e) => p.onChange(e.currentTarget.value)} />
        </Match>
        <Match when={kind() === "number"}>
          <input
            id={id()}
            type="number"
            min={p.schema?.minimum}
            max={p.schema?.maximum}
            step={p.schema?.type === "integer" ? 1 : "any"}
            value={p.value === undefined ? "" : String(p.value)}
            onChange={(e) => {
              const input = e.currentTarget;
              const n = toNumber(p.schema, input.value);
              if (input.value.trim() === "") p.onChange(undefined);
              else if (n !== undefined) p.onChange(n);
              // Show the value as it now is: clamped, or unchanged when the edit was refused.
              queueMicrotask(() => (input.value = p.value === undefined ? "" : String(p.value)));
            }}
          />
        </Match>
        <Match when={kind() === "boolean"}>
          <input id={id()} type="checkbox" checked={Boolean(p.value)} onChange={(e) => p.onChange(e.currentTarget.checked)} />
        </Match>
        <Match when={kind() === "select"}>
          <select id={id()} onChange={(e) => p.onChange(e.currentTarget.value === "" ? undefined : p.schema!.enum![e.currentTarget.selectedIndex - 1])}>
            <option value="" selected={p.value === undefined}>(default)</option>
            <For each={p.schema!.enum}>{(option) => <option selected={option === p.value}>{String(option)}</option>}</For>
          </select>
        </Match>
        <Match when={kind() === "list"}>
          <div class="list" id={id()}>
            {/* Index, not For: For keys items by value, so editing one would replace its input and lose focus. */}
            <Index each={(p.value as string[] | undefined) ?? []}>
              {(item, i) => (
                <div class="item">
                  <input
                    aria-label={`${p.name} ${i + 1}`}
                    value={item()}
                    onInput={(e) => p.onChange((p.value as string[]).map((v, j) => (j === i ? e.currentTarget.value : v)))}
                  />
                  <button title="Remove" onClick={() => p.onChange((p.value as string[]).filter((_, j) => j !== i))}>✕</button>
                </div>
              )}
            </Index>
            <button class="link" onClick={() => p.onChange([...((p.value as string[] | undefined) ?? []), ""])}>+ Add</button>
          </div>
        </Match>
        <Match when={kind() === "json"}>
          <textarea
            id={id()}
            class={jsonError() ? "invalid" : ""}
            rows={Math.min(8, JSON.stringify(p.value ?? null, null, 2).split("\n").length + 1)}
            value={p.value === undefined ? "" : JSON.stringify(p.value, null, 2)}
            onChange={(e) => {
              const text = e.currentTarget.value.trim();
              try {
                p.onChange(text === "" ? undefined : JSON.parse(text));
                setJsonError(false);
              } catch {
                setJsonError(true);
              }
            }}
          />
        </Match>
      </Switch>
      <Show when={p.schema?.description}>
        <p class="muted">{p.schema!.description}</p>
      </Show>
    </div>
  );
}
