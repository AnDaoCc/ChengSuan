import { it, expect } from 'vitest';
import { parseRateResponse } from './exchange-rates';
import { newPlan, newRule, planSchema } from '../core/model';
import { calculate } from '../core/calculator';

const snapshot = { source:'USD',target:'CNY',rate:'6.6874',date:'2026-10-13',fetchedAt:'2026-10-13T00:00:00Z',provider:'Frankfurter / 综合参考汇率（75 个来源）' };
it('accepts compact attribution and calculates the user subscription example', () => {
  const rates = parseRateResponse([snapshot]);
  const plan = newPlan();
  plan.items = [{ ...newRule('fixed','USD'),kind:'fixed',amount:'20',period:'month' }];
  plan.rates = rates;
  const saved = planSchema.parse(JSON.parse(JSON.stringify(plan)));
  expect(calculate(saved).complete).toBe(true);
  expect(calculate(saved).total.month).toBe('133.748');
  expect(saved.rates[0].provider).toBe(snapshot.provider);
});
it('rejects overlong attribution with a readable Chinese error, keeping validation enabled', () => {
  expect(() => parseRateResponse([{...snapshot,provider:'x'.repeat(201)}])).toThrow('汇率来源');
  try { parseRateResponse([{...snapshot,provider:'x'.repeat(201)}]); } catch(error) { expect((error as Error).message).not.toContain('too_big'); }
});
it('rejects missing and zero rates without changing existing snapshots', () => {
  const previous = structuredClone(snapshot);
  expect(() => parseRateResponse([{...snapshot,rate:'0'}])).toThrow('汇率数值');
  expect(() => parseRateResponse([{...snapshot,date:'invalid'}])).toThrow('汇率日期');
  expect(snapshot).toEqual(previous);
});
