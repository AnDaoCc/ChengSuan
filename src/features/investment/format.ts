import { D } from "../../core/investment/engine";
export function cash(value: string | null) {
  if (value === null) return "—";
  return "¥ " + new D(value).toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
export function price(value: string | null) {
  return value === null ? "—" : new D(value).toSignificantDigits(10).toFixed();
}
export function signed(value: string | null, percent = false) {
  if (value === null) return "—";
  const n = new D(value);
  return (n.gte(0) ? "+" : "") + n.toFixed(2) + (percent ? "%" : "");
}
export function direction(value: string | null) {
  return value === null
    ? ""
    : new D(value).gte(0)
      ? "market-up"
      : "market-down";
}
export function timestamp(value: string) {
  if (!value) return "未提供";
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? value
    : date.toLocaleString("zh-CN", {
        timeZone: "Asia/Shanghai",
        hour12: false,
      });
}
export const quoteStatus = {
  live: "最新参考",
  closed: "休市／收盘",
  cached: "缓存报价",
  manual: "手动估价",
  delayed: "延迟参考",
  missing: "报价缺失",
};
