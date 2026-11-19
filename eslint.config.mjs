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
      "desktop/**",
      "backend/**",
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

const tauriDynamicImportSyntaxRule = {
  selector: "ImportExpression[source.value=/^@tauri-apps\\/(api|plugin-)/]",
  message:
    "Dynamic imports of Tauri runtime APIs are restricted to frontend/src/services/**. Use a service method instead.",
};

const rawTruncationPattern = String.raw`\b(?:truncate|text-ellipsis|line-clamp-[123])\b`;
const rawTruncationMessage =
  "Raw truncation classes (`truncate`, `text-ellipsis`, `line-clamp-1..3`) are forbidden outside TruncatedText/UI primitives. Use <TruncatedText> from `@/components/foundation/TruncatedText` instead.";

const rawTruncationSyntaxRules = [
  {
    selector: `JSXAttribute[name.name=/^(?:className|.*ClassName)$/] Literal[value=/${rawTruncationPattern}/]`,
    message: rawTruncationMessage,
  },
  {
    selector: `JSXAttribute[name.name=/^(?:className|.*ClassName)$/] TemplateElement[value.raw=/${rawTruncationPattern}/]`,
    message: rawTruncationMessage,
  },
  {
    selector: `CallExpression[callee.name=/^(?:cn|clsx|cva)$/] Literal[value=/${rawTruncationPattern}/]`,
    message: rawTruncationMessage,
  },
  {
    selector: `CallExpression[callee.name=/^(?:cn|clsx|cva)$/] TemplateElement[value.raw=/${rawTruncationPattern}/]`,
    message: rawTruncationMessage,
  },
];

const rawTruncationIgnores = [
  "frontend/src/components/foundation/TruncatedText.tsx",
  "frontend/src/components/ui/**",
  "frontend/src/**/*.test.{ts,tsx}",
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
      "no-restricted-syntax": ["error", tauriDynamicImportSyntaxRule],
    },
  },
  {
    files: ["frontend/src/**/*.{ts,tsx}"],
    ignores: rawTruncationIgnores,
    rules: {
      "no-restricted-syntax": ["error", ...rawTruncationSyntaxRules],
    },
  },
  {
    files: ["frontend/src/**/*.{ts,tsx}"],
    ignores: ["frontend/src/services/**", ...rawTruncationIgnores],
    rules: {
      "no-restricted-syntax": [
        "error",
        tauriDynamicImportSyntaxRule,
        ...rawTruncationSyntaxRules,
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
