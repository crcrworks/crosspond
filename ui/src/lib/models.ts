export const EFFORTS = ['none', 'low', 'medium', 'high', 'xhigh'] as const;

export const CUSTOM_MODEL = '__custom__';

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
