import { z } from 'zod';
import { rateSchema } from '../core/model';

export function parseRateResponse(value: unknown) {
  const parsed = z.array(rateSchema).safeParse(value);
  if (!parsed.success) {
    const fields = [...new Set(parsed.error.issues.map(issue => String(issue.path.at(-1) ?? 'response')))];
    const labels: Record<string,string> = { provider:'汇率来源',rate:'汇率数值',date:'汇率日期',source:'原币种',target:'汇总币种',fetchedAt:'获取时间' };
    throw new Error(`返回的汇率数据格式不符合要求（${fields.map(field => labels[field] ?? '响应内容').join('、')}）。原汇率已保留，请重试或手动填写。`);
  }
  return parsed.data;
}
