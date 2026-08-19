<script lang="ts">
	import {
		collapsedLabel,
		liveActivityLabel,
		thoughtLabel,
		workHeaderVisual,
		workHostsPreparing,
		type TranscriptBlock,
		type WorkStep
	} from '$lib/transcript';
	import { toolRowLabel, toolVisual } from '$lib/tools';
	import ActivityLabel from './ActivityLabel.svelte';
	import Chevron from './Chevron.svelte';
	import Markdown from './Markdown.svelte';
	import ToolChip from './ToolChip.svelte';

	let {
		blocks,
		thinkingLiveIndex,
		preparing = false,
		ontoggle,
		ontogglestep
	}: {
		blocks: TranscriptBlock[];
		thinkingLiveIndex: number | null;
		preparing?: boolean;
		ontoggle: (index: number) => void;
		ontogglestep: (block: number, step: number) => void;
	} = $props();
</script>

<div class="flex flex-col gap-2">
	{#each blocks as block, index (index)}
		{#if block.kind === 'user'}
			<div
				class="w-full py-1 text-sm leading-[1.35] break-words text-[var(--muted)]"
				data-tauri-drag-region="false"
			>
				{block.text}
			</div>
		{:else if block.kind === 'text'}
			<Markdown source={block.text} />
		{:else}
			{@const header = workHeaderVisual(block.steps)}
			{@const showPreparing = preparing && workHostsPreparing(block)}
			<div class="flex flex-col gap-2">
				<button
					type="button"
					class="group flex w-full cursor-pointer flex-row items-center justify-start gap-1.5 appearance-none border-0 bg-transparent p-0 text-left"
					onclick={() => ontoggle(index)}
				>
					{#if header}
						<ToolChip src={header.icon} tone={header.tone} running={block.workedMs === null} />
					{/if}
					<div
						class="min-w-0 overflow-hidden text-left text-sm text-[var(--muted)] group-hover:opacity-80"
					>
						<ActivityLabel
							text={collapsedLabel(block, thinkingLiveIndex === index)}
							running={block.workedMs === null}
						/>
					</div>
					<Chevron expanded={block.expanded} />
				</button>
				{#if block.expanded}
					<div class="flex flex-col gap-2">
						{#each block.steps as step, row (row)}
							{@render workStep(index, row, step, thinkingLiveIndex === index, true)}
						{/each}
						{#if showPreparing}
							<div class="pl-4 text-sm text-[var(--muted)]">
								<ActivityLabel text={liveActivityLabel({ kind: 'preparing' })} running />
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{/if}
	{/each}
</div>

{#snippet workStep(
	blockIndex: number,
	row: number,
	step: WorkStep,
	thinkingLive: boolean,
	nested: boolean
)}
	{#if step.kind === 'thinking'}
		{@const live = thinkingLive && step.durationMs === null}
		<div class={['flex flex-col gap-2', nested && 'pl-4']}>
			<button
				type="button"
				class="group flex w-full cursor-pointer flex-row items-center justify-start gap-1 appearance-none border-0 bg-transparent p-0 text-left"
				onclick={() => ontogglestep(blockIndex, row)}
			>
				<div
					class="min-w-0 overflow-hidden text-left text-sm text-[var(--muted)] group-hover:opacity-80"
				>
					<ActivityLabel
						text={thoughtLabel(step.durationMs, step.startedAt, live)}
						running={live}
					/>
				</div>
				<Chevron expanded={step.expanded} />
			</button>
			{#if step.expanded && step.text.trim()}
				<div
					class="pl-4 text-sm leading-[1.35] break-words text-[var(--muted)]"
					data-tauri-drag-region="false"
				>
					{step.text.trim()}
				</div>
			{/if}
		</div>
	{:else}
		{@const visual = toolVisual(step.tool.name)}
		<div class={['flex w-full flex-row items-center gap-1.5', nested && 'pl-4']}>
			<ToolChip src={visual.icon} tone={visual.tone} running={step.tool.running} />
			<div class="min-w-0 flex-1 overflow-hidden text-sm text-[var(--muted)]">
				<ActivityLabel
					text={toolRowLabel(step.tool.name, step.tool.summary)}
					running={step.tool.running}
				/>
			</div>
		</div>
	{/if}
{/snippet}
