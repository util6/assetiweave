import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import eslintConfigPrettier from "eslint-config-prettier";

const isArchitectureOnly = process.env.ESLINT_ARCHITECTURE_ONLY === "1";

const baseConfig = [
  {
    ignores: [
      "dist/**",
      "build/**",
      "target/**",
      "cli/**",
      "src-tauri/**",
      "coverage/**",
      "node_modules/**",
      "agent-docs/**",
      "docs/**",
      "scripts/**",
    ],
  },
];

const generalConfig = isArchitectureOnly
  ? [tseslint.configs.base]
  : [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      {
        files: ["frontend/src/**/*.{ts,tsx}"],
        plugins: {
          "react-hooks": reactHooks,
        },
        rules: {
          "react-hooks/rules-of-hooks": "error",
          "react-hooks/exhaustive-deps": "warn",
          "@typescript-eslint/no-explicit-any": "off",
          "@typescript-eslint/no-unused-vars": [
            "warn",
            {
              argsIgnorePattern: "^_",
              varsIgnorePattern: "^_",
              caughtErrorsIgnorePattern: "^_",
            },
          ],
          "@typescript-eslint/no-empty-object-type": "off",
          "no-control-regex": "off",
          "preserve-caught-error": "off",
          "no-useless-assignment": "off",
          "no-useless-escape": "warn",
        },
      },
    ];

const architectureRulesConfig = [
  {
    files: ["frontend/src/**/*.{ts,tsx,js,mjs}"],
    ignores: ["frontend/src/services/**"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: [
                "@tauri-apps/api/core",
                "@tauri-apps/api/event",
                "@tauri-apps/api/window",
                "@tauri-apps/api/app",
                "@tauri-apps/plugin-*",
              ],
              message:
                "Direct runtime calls to Tauri are restricted to frontend/src/services/**. Use a service method instead.",
              allowTypeImports: true,
            },
          ],
        },
      ],
      "no-restricted-syntax": [
        "error",
        {
          selector:
            "ImportExpression[source.value=/^@tauri-apps\\/(api|plugin-)/]",
          message:
            "Dynamic imports of Tauri runtime APIs are restricted to frontend/src/services/**. Use a service method instead.",
        },
      ],
    },
  },
];

const prettierConfig = isArchitectureOnly ? [] : [eslintConfigPrettier];

export default tseslint.config(
  ...baseConfig,
  ...generalConfig,
  ...architectureRulesConfig,
  ...prettierConfig,
);
