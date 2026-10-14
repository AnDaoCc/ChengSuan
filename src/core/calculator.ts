import Decimal from 'decimal.js';
import { planSchema, type CostPlan, type CalculationResult, type PeriodTotals, type Period, type ItemResult, periodLabels } from './model';

const D = Decimal.clone({ precision: 100, rounding: Decimal.ROUND_HALF_UP });
const factors = { day: new D(365), week: new D(365).div(7), month: new D(12), year: new D(1) };
const periods = (annual: Decimal): PeriodTotals => Object.fromEntries(Object.entries(factors).map(([p, f]) => [p, annual.div(f).toFixed()])) as PeriodTotals;
const freeze = <T>(value: T): T => { if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); } return value; };
export function calculate(input: CostPlan): CalculationResult {
  const plan = planSchema.parse(input);
  let recurring = new D(0), amortized = new D(0), upfront = new D(0);
  const missing = new Set<string>();
  const items: ItemResult[] = plan.items.map(item => {
    let annual = new D(0), oneTime = new D(0), explanation = '';
    if (item.kind === 'fixed') { annual = new D(item.amount).mul(factors[item.period]); explanation = `${item.amount} ${item.currency} / ${periodLabels[item.period]}，年化倍数 ${factors[item.period].toFixed()}`; }
    if (item.kind === 'usage') {
      const quantity = new D(item.quantity).mul(new D(1).plus(new D(item.extraPercent).div(100)));
      let units = quantity.div(item.billingQuantity); if (item.billing === 'ceil') units = units.ceil();
      annual = units.mul(item.unitPrice).mul(item.count).mul(factors[item.period]);
      explanation = `每${item.quantityMode === 'use' ? '次' : '份'} ${item.quantity} ${item.unit} × (1 + ${item.extraPercent}%) ÷ ${item.billingQuantity}${item.billing === 'ceil' ? '，每次向上取整' : '，按比例计费'} × ${item.unitPrice} ${item.currency} × 每${periodLabels[item.period]} ${item.count} ${item.quantityMode === 'use' ? '次' : '份'}；年化 × ${factors[item.period].toFixed()}`;
    }
    if (item.kind === 'oneTime') {
      oneTime = new D(item.amount);
      if (item.amortization) { annual = oneTime.mul(factors[item.amortization.period]).div(item.amortization.duration); explanation = `${item.date} 一次性 ${item.amount} ${item.currency}，分摊 ${item.amortization.duration} ${periodLabels[item.amortization.period]}（等效平均，非实际账单）`; }
      else explanation = `${item.date} 一次性 ${item.amount} ${item.currency}；未分摊，不计入周期成本`;
    }
    const rate = item.currency === plan.currency ? null : plan.rates.find(r => r.source === item.currency && r.target === plan.currency) ?? null;
    const missingRate = item.currency !== plan.currency && !rate;
    if (missingRate) missing.add(item.currency);
    const converted = missingRate ? null : annual.mul(rate?.rate ?? '1');
    const convertedUpfront = missingRate ? null : oneTime.mul(rate?.rate ?? '1');
    if (converted) { if (item.kind === 'oneTime') amortized = amortized.plus(converted); else recurring = recurring.plus(converted); }
    if (convertedUpfront) upfront = upfront.plus(convertedUpfront);
    return { id: item.id, name: item.name, kind: item.kind, originalCurrency: item.currency, rate, originalAnnual: annual.toFixed(), convertedAnnual: converted?.toFixed() ?? null, periods: converted ? periods(converted) : null, upfrontOriginal: oneTime.toFixed(), upfrontConverted: convertedUpfront?.toFixed() ?? null, explanation, missingRate };
  });
  return freeze({ version: 1, generatedAt: new Date().toISOString(), plan, currency: plan.currency, complete: !missing.size, missingCurrencies: [...missing], recurring: periods(recurring), amortized: periods(amortized), total: periods(recurring.plus(amortized)), upfront: upfront.toFixed(), items, methodology: '等效平均成本：一年按 365 天、12 个月，一周 7 天。每次使用或每份产出独立取整。一次性费用仅明确启用分摊时计入周期成本。参考汇率不包含银行手续费。' });
}
export function money(value: string, currency: string, precision: number | null = null): string {
  const digits = precision ?? new Intl.NumberFormat('zh-CN', { style: 'currency', currency }).resolvedOptions().maximumFractionDigits ?? 2;
  const [integer, fraction] = new D(value).toFixed(digits).split('.');
  return `${currency} ${integer.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}${fraction === undefined ? '' : `.${fraction}`}`;
}
export function periodValue(amount: string, from: Period, to: Period): string { return new D(amount).mul(factors[from]).div(factors[to]).toFixed(); }
