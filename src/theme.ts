/**
 * 界面主题：浅色（Alabaster Precision）、深色（Obsidian Precision），默认跟随系统。
 * 依据视觉设计规范第 2 节：跟随系统时不写 data-theme，由 styles.css 里的
 * @media (prefers-color-scheme: dark) 生效；手动选择时写入 <html data-theme="light|dark"> 覆盖。
 * 选择保存在本机 localStorage，不写入工作区配置。
 */
import { reactive } from "vue";

export type ThemePref = "system" | "light" | "dark";

const STORAGE_KEY = "repounion.theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");

export const theme = reactive({
  pref: "system" as ThemePref,
  /** 实际生效的明暗，供需要区分明暗的组件使用。 */
  dark: false,
});

function readPref(): ThemePref {
  const saved = window.localStorage.getItem(STORAGE_KEY);
  return saved === "light" || saved === "dark" || saved === "system" ? saved : "system";
}

function apply() {
  const root = document.documentElement;
  if (theme.pref === "system") {
    delete root.dataset.theme;
    theme.dark = media.matches;
  } else {
    root.dataset.theme = theme.pref;
    theme.dark = theme.pref === "dark";
  }
}

/** 在挂载应用前调用，避免启动时先闪一下另一套配色。 */
export function initTheme() {
  theme.pref = readPref();
  apply();
  media.addEventListener("change", () => {
    if (theme.pref === "system") {
      apply();
    }
  });
}

export function setTheme(pref: ThemePref) {
  theme.pref = pref;
  window.localStorage.setItem(STORAGE_KEY, pref);
  apply();
}
