import type { Language } from "@/lib/types";
import { enUS } from "./en-US";
import { zhCN, type MessageKey } from "./zh-CN";

export type { MessageKey };

const dictionaries: Record<Language, Partial<Record<MessageKey, string>>> = {
  "zh-CN": zhCN,
  "en-US": enUS,
};

/** 翻译函数：缺失英文键回退中文。支持 `{name}` 占位符。 */
export function translate(
  language: Language,
  key: MessageKey,
  vars?: Record<string, string | number>,
): string {
  const dictionary = dictionaries[language] ?? zhCN;
  let text = dictionary[key] ?? zhCN[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}
