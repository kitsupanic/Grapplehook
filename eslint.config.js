import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist/', 'src-tauri/', 'pnpm-lock.yaml', '.impeccable/'] },
  js.configs.recommended,
  tseslint.configs.recommended,
  svelte.configs.recommended,
  { languageOptions: { globals: globals.browser } },
  { files: ['**/*.svelte', '**/*.svelte.ts'], languageOptions: { parserOptions: { parser: tseslint.parser } } },
  { files: ['*.config.{js,ts}'], languageOptions: { globals: globals.node } },
);
