export const EFFORTS = ['none', 'low', 'medium', 'high', 'xhigh'] as const;

export const CUSTOM_MODEL = '__custom__';

const MODEL_OPTION_SEP = '\t';

export function effortLabel(effort: string): string {
	switch (effort) {
		case 'none':
			return 'none';
		case 'low':
			return 'low';
		case 'high':
			return 'high';
		case 'xhigh':
			return 'xhigh';
		default:
			return 'medium';
	}
}

export function modelOptionValue(source: string, model: string): string {
	return `${source}${MODEL_OPTION_SEP}${model}`;
}

export function parseModelOption(value: string): { source: string; model: string } {
	const split = value.indexOf(MODEL_OPTION_SEP);
	if (split < 0) {
		return { source: 'default', model: value };
	}
	return {
		source: value.slice(0, split),
		model: value.slice(split + 1)
	};
}

export function isCustomModelOption(model: string): boolean {
	return model === CUSTOM_MODEL;
}
