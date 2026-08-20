export type UpdateNotice = 'hidden' | 'available' | 'installing';

export function updateNoticeState(input: {
	available: boolean;
	dismissed: boolean;
	installing: boolean;
}): UpdateNotice {
	if (input.installing) return 'installing';
	if (input.available && !input.dismissed) return 'available';
	return 'hidden';
}
