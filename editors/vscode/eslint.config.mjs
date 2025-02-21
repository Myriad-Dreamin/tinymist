import globals from "globals";
import typescriptEslint from "@typescript-eslint/eslint-plugin";
import tsParser from "@typescript-eslint/parser";
import path from "node:path";
import { fileURLToPath } from "node:url";
import js from "@eslint/js";
import { FlatCompat } from "@eslint/eslintrc";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const compat = new FlatCompat({
  baseDirectory: __dirname,
  recommendedConfig: js.configs.recommended,
  allConfig: js.configs.all,
});

export default [
  {
    ignores: ["**/.eslintrc.js", "**/out/", "**/node_modules/"],
  },
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
  ...compat
    .extends(
      "eslint:recommended",
      "plugin:@typescript-eslint/eslint-recommended",
      "plugin:@typescript-eslint/recommended",
      "plugin:@typescript-eslint/recommended-requiring-type-checking",
    )
    .map((config) => ({
      ...config,
      files: ["**/*.ts"],
    })),
  {
    files: ["**/*.ts"],

    plugins: {
      "@typescript-eslint": typescriptEslint,
    },

    languageOptions: {
      parser: tsParser,
      ecmaVersion: 10,
      sourceType: "module",

      parserOptions: {
        project: ["./tsconfig.json"],

        ecmaFeatures: {
          modules: true,
        },
      },
    },

    rules: {
      "@typescript-eslint/member-delimiter-style": [
        "error",
        {
          multiline: {
            delimiter: "semi",
            requireLast: true,
          },

          singleline: {
            delimiter: "semi",
            requireLast: false,
          },
        },
      ],

      semi: [2, "always"],

      "@typescript-eslint/no-inferrable-types": [
        "error",
        {
          ignoreParameters: true,
          ignoreProperties: true,
        },
      ],

      "@typescript-eslint/ban-ts-comment": 0,
      "@typescript-eslint/no-empty-function": 0,
      "@typescript-eslint/no-var-requires": 0,
      "@typescript-eslint/no-explicit-any": 0,
      "@typescript-eslint/no-floating-promises": 0,
      "@typescript-eslint/no-unsafe-assignment": 0,
      "@typescript-eslint/no-unsafe-return": 0,
      "@typescript-eslint/no-unsafe-call": 0,
      "@typescript-eslint/no-unsafe-member-access": 0,
      "@typescript-eslint/unbound-method": 0,
    },
  },
];
