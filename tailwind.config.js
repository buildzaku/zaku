import { fontFamily } from "tailwindcss/defaultTheme";

/** @type {import('tailwindcss').Config} */
const config = {
    darkMode: ["class"],
    content: ["./src/**/*.{html,js,svelte,ts}"],
    safelist: ["dark"],
    theme: {
        extend: {
            fontSize: {
                tiny: [
                    "0.6875rem",
                    {
                        lineHeight: "1rem",
                        letterSpacing: "0em",
                        fontWeight: "400",
                    },
                ],
                small: [
                    "0.8125rem",
                    {
                        lineHeight: "1.25rem",
                        letterSpacing: "0em",
                        fontWeight: "400",
                    },
                ],
                medium: [
                    "0.9375rem",
                    {
                        lineHeight: "1.5rem",
                        letterSpacing: "0em",
                        fontWeight: "400",
                    },
                ],
                large: [
                    "1.0625rem",
                    {
                        lineHeight: "1.75rem",
                        letterSpacing: "0em",
                        fontWeight: "400",
                    },
                ],
            },
            colors: {
                border: "hsl(var(--border) / <alpha-value>)",
                input: "hsl(var(--input) / <alpha-value>)",
                ring: "hsl(var(--ring) / <alpha-value>)",
                background: "hsl(var(--background) / <alpha-value>)",
                foreground: "hsl(var(--foreground) / <alpha-value>)",
                primary: {
                    DEFAULT: "hsl(var(--primary) / <alpha-value>)",
                    foreground: "hsl(var(--primary-foreground) / <alpha-value>)",
                },
                secondary: {
                    DEFAULT: "hsl(var(--secondary) / <alpha-value>)",
                    foreground: "hsl(var(--secondary-foreground) / <alpha-value>)",
                },
                destructive: {
                    DEFAULT: "hsl(var(--destructive) / <alpha-value>)",
                    foreground: "hsl(var(--destructive-foreground) / <alpha-value>)",
                },
                muted: {
                    DEFAULT: "hsl(var(--muted) / <alpha-value>)",
                    foreground: "hsl(var(--muted-foreground) / <alpha-value>)",
                },
                accent: {
                    DEFAULT: "hsl(var(--accent) / <alpha-value>)",
                    foreground: "hsl(var(--accent-foreground) / <alpha-value>)",
                },
                popover: {
                    DEFAULT: "hsl(var(--popover) / <alpha-value>)",
                    foreground: "hsl(var(--popover-foreground) / <alpha-value>)",
                },
                card: {
                    DEFAULT: "hsl(var(--card) / <alpha-value>)",
                    foreground: "hsl(var(--card-foreground) / <alpha-value>)",
                },
                "method-get": {
                    DEFAULT: "hsl(var(--method-get) / <alpha-value>)",
                },
                "method-post": {
                    DEFAULT: "hsl(var(--method-post) / <alpha-value>)",
                },
                "method-put": {
                    DEFAULT: "hsl(var(--method-put) / <alpha-value>)",
                },
                "method-patch": {
                    DEFAULT: "hsl(var(--method-patch) / <alpha-value>)",
                },
                "method-delete": {
                    DEFAULT: "hsl(var(--method-delete) / <alpha-value>)",
                },
                "method-head": {
                    DEFAULT: "hsl(var(--method-head) / <alpha-value>)",
                },
                "method-options": {
                    DEFAULT: "hsl(var(--method-options) / <alpha-value>)",
                },
            },
            borderRadius: {
                lg: "var(--radius)",
                md: "calc(var(--radius) - 2px)",
                sm: "calc(var(--radius) - 4px)",
            },
            borderWidth: {
                DEFAULT: "0.5px",
            },
            fontFamily: {
                sans: [...fontFamily.sans],
            },
        },
    },
};

export default config;
