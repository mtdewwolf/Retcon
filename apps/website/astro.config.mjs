import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

export default defineConfig({
  site: "https://mtdewwolf.github.io",
  base: "/Retcon",
  output: "static",
  integrations: [sitemap()],
});
