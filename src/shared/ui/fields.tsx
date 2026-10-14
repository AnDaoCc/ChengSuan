import type {ReactNode} from 'react';import {currencies,PERIODS,periodLabels,type Period} from '../../core/model';
import {currencyLabel} from '../currency';
export function Field({label,hint,children}:{label:string;hint?:string;children:ReactNode}){return <label className="field"><span>{label}</span>{children}{hint&&<small>{hint}</small>}</label>;}
export function CurrencySelect({value,onChange}:{value:string;onChange:(s:string)=>void}){return <select value={value} onChange={e=>onChange(e.target.value)}>{[...new Set(['CNY','USD','EUR','HKD','JPY','GBP','AUD','CAD',...currencies])].map(c=><option key={c} value={c}>{currencyLabel(c)}</option>)}</select>;}
export function PeriodSelect({value,onChange}:{value:Period;onChange:(s:Period)=>void}){return <select value={value} onChange={e=>onChange(e.target.value as Period)}>{PERIODS.map(p=><option key={p} value={p}>每{periodLabels[p]}</option>)}</select>;}
