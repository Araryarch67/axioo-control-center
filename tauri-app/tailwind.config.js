/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        border: "rgb(var(--border) / <alpha-value>)",
        hair: "rgb(var(--hair) / <alpha-value>)",
        background: "rgb(var(--bg) / <alpha-value>)",
        bg2: "rgb(var(--bg2) / <alpha-value>)",
        card: "rgb(var(--card) / <alpha-value>)",
        card2: "rgb(var(--card2) / <alpha-value>)",
        ink: "rgb(var(--ink) / <alpha-value>)",
        cream: "rgb(var(--cream) / <alpha-value>)",
        dim: "rgb(var(--dim) / <alpha-value>)",
        faint: "rgb(var(--faint) / <alpha-value>)",
        ghost: "rgb(var(--ghost) / <alpha-value>)",
        accent: "rgb(var(--accent) / <alpha-value>)",
        lime: "rgb(var(--lime) / <alpha-value>)",
        ok: "rgb(var(--ok) / <alpha-value>)",
        warn: "rgb(var(--accent) / <alpha-value>)",
        bad: "rgb(var(--bad) / <alpha-value>)",
      },
      fontFamily: {
        mono: ['"Iosevka Nerd Font Mono"', '"Iosevka Nerd Font"', "monospace"],
        sans: ['"Iosevka Nerd Font"', '"Iosevka Nerd Font Mono"', "monospace"],
      },
    },
  },
  plugins: [],
};
