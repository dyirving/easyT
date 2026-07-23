// 工具函数
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** 合并 Tailwind 类名 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 生成简单唯一请求 ID（用于区分并发翻译请求） */
export function genRequestId(): string {
  return `req_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}
