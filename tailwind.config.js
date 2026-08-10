/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: {
          DEFAULT: "rgb(var(--color-surface) / <alpha-value>)",
          soft: "rgb(var(--color-surface-soft) / <alpha-value>)",
          panel: "rgb(var(--color-surface-panel) / <alpha-value>)",
        },
        ink: {
          DEFAULT: "rgb(var(--color-ink) / <alpha-value>)",
          soft: "rgb(var(--color-ink-soft) / <alpha-value>)",
          muted: "rgb(var(--color-ink-muted) / <alpha-value>)",
        },
        line: "rgb(var(--color-line) / <alpha-value>)",
        accent: "rgb(var(--color-accent) / <alpha-value>)",
        danger: "rgb(var(--color-danger) / <alpha-value>)",
        success: "rgb(var(--color-success) / <alpha-value>)",
        warning: "rgb(var(--color-warning) / <alpha-value>)",
      },
      fontFamily: {
        sans: ["var(--font-sans)"],
      },
      boxShadow: {
        soft: "var(--shadow-soft)",
      },
      borderRadius: {
        compact: "var(--radius-compact)",
        control: "var(--radius-control)",
        surface: "var(--radius-surface)",
        xl: "var(--radius-surface)",
      },
      maxWidth: {
        translation: "560px",
      },
    },
  },
  plugins: [],
};
