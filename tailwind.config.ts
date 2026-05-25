import type { Config } from "tailwindcss";

export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "#0b1326",
        surface: "#171f33",
        "surface-lowest": "#060e20",
        "surface-low": "#131b2e",
        "surface-card": "#1e293b",
        "surface-high": "#222a3d",
        "surface-highest": "#2d3449",
        border: "#334155",
        outline: "#8c909f",
        "outline-variant": "#424754",
        "on-surface": "#dae2fd",
        "on-surface-variant": "#c2c6d6",
        primary: "#adc6ff",
        "primary-strong": "#4d8eff",
        "status-update": "#0ea5e9",
        "status-create": "#10b981",
        "status-remove": "#f43f5e",
        "status-conflict": "#f59e0b",
      },
      borderRadius: {
        DEFAULT: "0.125rem",
        lg: "0.25rem",
        xl: "0.5rem",
        full: "0.75rem",
      },
      spacing: {
        "sidebar-width": "64px",
      },
      fontFamily: {
        sans: ["Geist", "sans-serif"],
        mono: ["JetBrains Mono", "monospace"],
      },
      fontSize: {
        "body-sm": ["12px", { lineHeight: "16px", fontWeight: "400" }],
        "body-md": ["14px", { lineHeight: "20px", fontWeight: "400" }],
        "code-md": ["13px", { lineHeight: "18px", fontWeight: "450" }],
        "label-caps": ["11px", { lineHeight: "16px", letterSpacing: "0.05em", fontWeight: "700" }],
        h2: ["18px", { lineHeight: "24px", fontWeight: "600" }],
      },
      boxShadow: {
        glow: "0 14px 28px rgba(14, 165, 233, 0.18)",
      },
    },
  },
  plugins: [],
} satisfies Config;
