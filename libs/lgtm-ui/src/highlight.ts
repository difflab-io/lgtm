import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import type { ThemedToken } from "shiki";
const languages = {
  rust: () => import("@shikijs/langs/rust"),
  go: () => import("@shikijs/langs/go"),
  typescript: () => import("@shikijs/langs/typescript"),
  javascript: () => import("@shikijs/langs/javascript"),
  tsx: () => import("@shikijs/langs/tsx"),
  json: () => import("@shikijs/langs/json"),
  markdown: () => import("@shikijs/langs/markdown"),
  python: () => import("@shikijs/langs/python"),
  toml: () => import("@shikijs/langs/toml"),
  yaml: () => import("@shikijs/langs/yaml"),
};
const extensions: Record<string, keyof typeof languages> = {
  rs: "rust",
  go: "go",
  ts: "typescript",
  tsx: "tsx",
  js: "javascript",
  json: "json",
  md: "markdown",
  py: "python",
  toml: "toml",
  yaml: "yaml",
  yml: "yaml",
};
let highlighter: ReturnType<typeof createHighlighterCore> | undefined;
export async function highlight(
  lines: string[],
  path: string,
): Promise<ThemedToken[][]> {
  const lang = extensions[path.split(".").at(-1) || ""];
  if (!lang) return lines.map((content) => [{ content, offset: 0 }]);
  highlighter ??= createHighlighterCore({
    themes: [import("@shikijs/themes/github-dark-default")],
    langs: [],
    engine: createJavaScriptRegexEngine(),
  });
  const h = await highlighter;
  await h.loadLanguage(languages[lang]());
  return h.codeToTokens(lines.join("\n"), {
    lang,
    theme: "github-dark-default",
  }).tokens;
}
