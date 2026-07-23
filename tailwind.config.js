/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // 浅色暖灰背景 + 深灰正文，专注阅读
        // 结构预留 dark 主题，第一版仅实现 light
        surface: {
          DEFAULT: "#f6f5f2",
          soft: "#efece6",
          panel: "#ffffff",
        },
        ink: {
          DEFAULT: "#2f3136",
          soft: "#595c63",
          muted: "#8a8d94",
        },
        line: "#e3ded5",
        accent: "#5a7d8c",
        danger: "#b6553c",
        success: "#4f7a52",
      },
      fontFamily: {
        sans: [
          "system-ui",
          "-apple-system",
          "Segoe UI",
          "Microsoft YaHei",
          "PingFang SC",
          "Hiragino Sans GB",
          "sans-serif",
        ],
      },
      boxShadow: {
        soft: "0 6px 24px -8px rgba(60, 55, 45, 0.18)",
      },
      borderRadius: {
        xl: "12px",
      },
      maxWidth: {
        translation: "560px",
      },
    },
  },
  plugins: [],
};
