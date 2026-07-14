/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        hypr: {
          primary: '#22d3ee',
          bg: '#030712',
          card: '#111827',
          border: '#1f2937',
        }
      }
    },
  },
  plugins: [
    require('@tailwindcss/typography'),
  ],
}