/** Same palette as the web interface (apps/web/src/styles.css). */
module.exports = {
  content: ["./src/**/*.{ts,tsx}"],
  presets: [require("nativewind/preset")],
  theme: {
    extend: {
      colors: {
        bg: "#09090c",
        surface: "#131318",
        "surface-2": "#1c1c23",
        "surface-3": "#26262f",
        line: "#2c2c36",
        fg: "#f4f4f6",
        muted: "#a2a2b0",
        faint: "#6b6b78",
        accent: "#f4b740",
        "accent-strong": "#ffc94f",
        "accent-ink": "#1a1405",
        danger: "#f0616d",
        success: "#4ccf8f",
      },
      borderRadius: { card: "0.75rem" },
    },
  },
};
