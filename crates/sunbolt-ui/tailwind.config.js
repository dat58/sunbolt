/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.rs"],
  theme: {
    extend: {
      colors: {
        sun: {
          amber: "#FBBF24",
        },
        warm: {
          orange: "#F59E0B",
        },
        electric: {
          violet: "#7C3AED",
          blue: "#2563EB",
        },
        lightning: {
          cyan: "#22D3EE",
        },
        terminal: {
          bg: "#09090B",
          surface: "#18181B",
          border: "#27272A",
          text: "#FAFAFA",
          muted: "#A1A1AA",
        },
      },
      fontFamily: {
        sans: [
          "Inter",
          "ui-sans-serif",
          "system-ui",
          "-apple-system",
          "BlinkMacSystemFont",
          "\"Segoe UI\"",
          "sans-serif",
        ],
        mono: [
          "ui-monospace",
          "SFMono-Regular",
          "Menlo",
          "Consolas",
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
