import clsx, { type ClassValue } from "clsx";

/** 合并 className（clsx 简版）。 */
export function cn(...inputs: ClassValue[]): string {
  return clsx(inputs);
}
