import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import parsh from "./src/lib/parsh-grammar.ts";

export default defineConfig({
  site: "https://jmjoy.github.io",
  base: "/parsh",
  trailingSlash: "always",
  integrations: [
    starlight({
      title: "parsh",
      description: "A shell built around S-expressions.",
      defaultLocale: "en",
      locales: {
        en: { label: "English", lang: "en" },
        "zh-cn": { label: "简体中文", lang: "zh-CN" },
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/jmjoy/parsh",
        },
      ],
      sidebar: [
        {
          label: "Start here",
          translations: { "zh-CN": "开始使用" },
          items: [
            {
              slug: "docs",
              label: "Introduction",
              translations: { "zh-CN": "项目介绍" },
            },
            {
              slug: "docs/getting-started",
              label: "Getting started",
              translations: { "zh-CN": "快速开始" },
            },
          ],
        },
        {
          label: "The language",
          translations: { "zh-CN": "语言指南" },
          items: [
            {
              slug: "docs/syntax",
              label: "Syntax & values",
              translations: { "zh-CN": "语法与值" },
            },
            {
              slug: "docs/variables",
              label: "Variables & scope",
              translations: { "zh-CN": "变量与作用域" },
            },
            {
              slug: "docs/control-flow",
              label: "Functions & control flow",
              translations: { "zh-CN": "函数与控制流" },
            },
            {
              slug: "docs/commands",
              label: "Commands & composition",
              translations: { "zh-CN": "命令与组合" },
            },
            {
              slug: "docs/structs-and-macros",
              label: "Structs & macros",
              translations: { "zh-CN": "结构与宏" },
            },
          ],
        },
        {
          label: "Reference",
          translations: { "zh-CN": "参考" },
          items: [
            {
              slug: "docs/interactive",
              label: "Interactive editing",
              translations: { "zh-CN": "交互编辑" },
            },
            {
              slug: "docs/diagnostics",
              label: "Errors & exit codes",
              translations: { "zh-CN": "错误与退出码" },
            },
            {
              slug: "docs/playground",
              label: "Playground",
              translations: { "zh-CN": "在线体验终端" },
            },
            {
              slug: "docs/development",
              label: "Development",
              translations: { "zh-CN": "开发指南" },
            },
          ],
        },
      ],
      components: { Header: "./src/components/DocsHeader.astro" },
      customCss: ["./src/styles/docs.css"],
      expressiveCode: { shiki: { langs: [parsh] } },
    }),
  ],
});
