import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri/target", "src-tauri/gen", "src/types/ir.ts"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ["*.config.{js,ts}", "scripts/**/*.mjs"],
    languageOptions: { globals: globals.node },
  },
  prettier,
);
