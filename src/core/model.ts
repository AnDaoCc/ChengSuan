import { z } from 'zod';
import Decimal from 'decimal.js';

export const PERIODS = ['day', 'week', 'month', 'year'] as const;
export type Period = typeof PERIODS[number];
export const periodLabels: Record<Period, string> = { day: '日', week: '周', month: '月', year: '年' };
export const decimal = z.string().regex(/^\d+(\.\d+)?$/, '请输入非负十进制数').max(80).refine(v => new Decimal(v).isFinite(), '数值无效');
const positive = decimal.refine(v => new Decimal(v).gt(0), '必须大于 0');
export const currency = z.string().regex(/^[A-Z]{3}$/, '请选择有效币种');
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/, '请选择日期').refine(v => !Number.isNaN(Date.parse(v)) && new Date(v).toISOString().slice(0, 10) === v, '日期无效');
const period = z.enum(PERIODS);
const common = { id: z.string().uuid(), name: z.string().trim().min(1, '请输入项目名称').max(200), currency, note: z.string().max(4000) };
export const ruleSchema = z.discriminatedUnion('kind', [
  z.object({ ...common, kind: z.literal('fixed'), amount: decimal, period }),
  z.object({ ...common, kind: z.literal('usage'), unitPrice: decimal, billingQuantity: positive, unit: z.string().trim().min(1, '请输入单位').max(80), quantity: decimal, quantityMode: z.enum(['use', 'output']), count: decimal, period, billing: z.enum(['proportional', 'ceil']), extraPercent: decimal }),
  z.object({ ...common, kind: z.literal('oneTime'), amount: decimal, date, amortization: z.object({ duration: positive, period }).nullable() }),
]);
export type CostRule = z.infer<typeof ruleSchema>;
export const rateSchema = z.object({ source: currency, target: currency, rate: positive, date, fetchedAt: z.string().datetime(), provider: z.string().min(1).max(200) });
export type ExchangeRateSnapshot = z.infer<typeof rateSchema>;
export const planSchema = z.object({ schemaVersion: z.literal(1), id: z.string().uuid(), name: z.string().trim().min(1, '请输入方案名称').max(200), description: z.string().max(4000), currency, items: z.array(ruleSchema).max(10000), rates: z.array(rateSchema).max(300), createdAt: z.string().datetime(), updatedAt: z.string().datetime() }).superRefine((p, ctx) => {
  if (new Set(p.items.map(i => i.id)).size !== p.items.length) ctx.addIssue({ code: 'custom', message: '项目 ID 重复', path: ['items'] });
  if (new Set(p.rates.map(r => r.source)).size !== p.rates.length || p.rates.some(r => r.target !== p.currency)) ctx.addIssue({ code: 'custom', message: '汇率重复或目标币种不匹配', path: ['rates'] });
});
export type CostPlan = z.infer<typeof planSchema>;
export const settingsSchema = z.object({ theme: z.enum(['light', 'dark', 'system']), defaultCurrency: currency, precision: z.number().int().min(0).max(8).nullable(), lastFormat: z.enum(['text', 'xlsx', 'csv', 'chengsuan']), lastScope: z.enum(['current', 'all']), lastDetail: z.enum(['summary', 'full']) });
export type Settings = z.infer<typeof settingsSchema>;
export const defaultSettings: Settings = { theme: 'system', defaultCurrency: 'CNY', precision: null, lastFormat: 'xlsx', lastScope: 'current', lastDetail: 'full' };
export type PeriodTotals = Record<Period, string>;
export type ItemResult = { id: string; name: string; kind: CostRule['kind']; originalCurrency: string; rate: ExchangeRateSnapshot | null; originalAnnual: string; convertedAnnual: string | null; periods: PeriodTotals | null; upfrontOriginal: string; upfrontConverted: string | null; explanation: string; missingRate: boolean };
export type CalculationResult = Readonly<{ version: 1; generatedAt: string; plan: CostPlan; currency: string; complete: boolean; missingCurrencies: string[]; recurring: PeriodTotals; amortized: PeriodTotals; total: PeriodTotals; upfront: string; items: ItemResult[]; methodology: string }>;
export interface PlanRepository { load(): Promise<{ plans: CostPlan[]; settings: Settings; dataDirectory: string }>; savePlan(plan: CostPlan): Promise<void>; deletePlan(id: string): Promise<void>; saveSettings(settings: Settings): Promise<void>; importPlans(plans: CostPlan[]): Promise<void> }
export const currencies = Intl.supportedValuesOf('currency');
export function newPlan(currencyCode = 'CNY'): CostPlan { const now = new Date().toISOString(); return { schemaVersion: 1, id: crypto.randomUUID(), name: '未命名方案', description: '', currency: currencyCode, items: [], rates: [], createdAt: now, updatedAt: now }; }
export function newRule(kind: CostRule['kind'], currencyCode: string): CostRule { const base = { id: crypto.randomUUID(), name: '新成本项目', currency: currencyCode, note: '' }; if (kind === 'fixed') return { ...base, kind, amount: '0', period: 'month' }; if (kind === 'oneTime') return { ...base, kind, amount: '0', date: new Date().toLocaleDateString('en-CA'), amortization: null }; return { ...base, kind, unitPrice: '0', billingQuantity: '1', unit: '份', quantity: '1', quantityMode: 'use', count: '1', period: 'month', billing: 'proportional', extraPercent: '0' }; }
