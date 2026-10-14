import * as React from 'react';import {cva,type VariantProps} from 'class-variance-authority';import {cn} from '../utils';
const variants=cva('button',{variants:{variant:{default:'button-primary',secondary:'button-secondary',ghost:'button-ghost',destructive:'button-danger'}},defaultVariants:{variant:'default'}});
export function Button({className,variant,...props}:React.ButtonHTMLAttributes<HTMLButtonElement>&VariantProps<typeof variants>){return <button className={cn(variants({variant}),className)} {...props}/>;}
