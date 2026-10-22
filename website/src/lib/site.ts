export type Locale = "en" | "zh-cn";
export const locales: Locale[] = ["en", "zh-cn"];
export const base = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;
export const url = (locale: Locale, path = "") =>
  `${base}${locale}/${path.replace(/^\//, "")}`;
export const localeOf = (path: string): Locale =>
  path.includes("/zh-cn/") ? "zh-cn" : "en";
export const words = {
  en: {
    home: "Home",
    playground: "Playground",
    docs: "Docs",
    github: "GitHub",
    theme: "Toggle color theme",
    skip: "Skip to content",
    footer: "A shell built around S-expressions.",
    license: "MulanPSL 2.0",
    source: "Source code",
  },
  "zh-cn": {
    home: "首页",
    playground: "体验终端",
    docs: "文档",
    github: "GitHub",
    theme: "切换明暗主题",
    skip: "跳至正文",
    footer: "一个围绕 S 表达式构建的 Shell。",
    license: "木兰宽松许可证，第 2 版",
    source: "源代码",
  },
} satisfies Record<Locale, Record<string, string>>;
