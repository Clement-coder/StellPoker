import type { Preview } from "@storybook/react-webpack5";
import "../src/app/globals.css";

const preview: Preview = {
  parameters: {
    backgrounds: {
      default: "poker-table",
      values: [
        { name: "poker-table", value: "#1a5c2a" },
        { name: "dark", value: "#0c0a18" },
        { name: "light", value: "#f5e6c8" },
      ],
    },
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
  },
};

export default preview;
