<script lang="ts">
	import {
		collapsedLabel,
		thoughtLabel,
		workHeaderIcon,
		type TranscriptBlock,
		type WorkStep
	} from '$lib/transcript';
	import { toolRowLabel } from '$lib/tools';
	import ActivityLabel from './ActivityLabel.svelte';
	import Icon from './Icon.svelte';
	import Markdown from './Markdown.svelte';

	let {
		blocks,
		thinkingLiveIndex,
		ontoggle,
		ontogglestep
	}: {
		blocks: TranscriptBlock[];
		thinkingLiveIndex: number | null;
		ontoggle: (index: number) => void;
		ontogglestep: (block: number, step: number) => void;
	} = $props();
</script>

{#each blocks as block, index (index)}
	{#if block.kind === 'user'}
		<div class="w-full py-1 text-sm leading-[1.35] break-words text-[var(--muted)]">{block.text}</div>
	{:else if block.kind === 'text'}
		<Markdown source={block.text} />
	{:else}
		{@const sealed = block.workedMs !== null}
		{#if !sealed}
			<div class="flex flex-col gap-1">
				{#each block.steps as step, row (row)}
					{@render workStep(index, row, step, thinkingLiveIndex === index, false)}
				{/each}
			</div>
		{:else}
			<button
				type="button"
				class="flex w-full cursor-pointer flex-row items-center gap-1 hover:opacity-80"
				onclick={() => ontoggle(index)}
			>
				<span class="shrink-0 text-sm text-[var(--muted)]">{block.expanded ? '▾' : '▸'}</span>
				{#if workHeaderIcon(block.steps)}
					<Icon src={workHeaderIcon(block.steps) ?? ''} />
				{/if}
				<div class="min-w-0 flex-1 overflow-hidden text-sm text-[var(--muted)]">
					<ActivityLabel text={collapsedLabel(block, false)} />
				</div>
			</button>
			{#if block.expanded}
				{#each block.steps as step, row (row)}
					{@render workStep(index, row, step, false, true)}
				{/each}
			{/if}
		{/if}
	{/if}
{/each}

{#snippet workStep(
	blockIndex: number,
	row: number,
	step: WorkStep,
	thinkingLive: boolean,
	nested: boolean
)}
	{#if step.kind === 'thinking'}
		{@const live = thinkingLive && step.durationMs === null}
		<div class={['flex flex-col gap-1', nested && 'pl-4']}>
			<button
				type="button"
				class="flex w-full cursor-pointer flex-row items-center gap-1 hover:opacity-80"
				onclick={() => ontogglestep(blockIndex, row)}
			>
				<span class="shrink-0 text-sm text-[var(--muted)]">{step.expanded ? '▾' : '▸'}</span>
				<div class="min-w-0 flex-1 overflow-hidden text-sm text-[var(--muted)]">
					<ActivityLabel
						text={thoughtLabel(step.durationMs, step.startedAt, live)}
						running={live}
					/>
				</div>
			</button>
			{#if step.expanded && step.text.trim()}
				<div class="pl-4 text-sm leading-[1.35] break-words text-[var(--muted)]">{step.text.trim()}</div>
			{/if}
		</div>
	{:else if step.kind === 'narration'}
		<div class={nested ? 'w-full pl-4' : 'w-full'}>
			<Markdown source={step.text} />
		</div>
	{:else}
		<div class={['flex w-full flex-row items-center gap-1', nested && 'pl-4']}>
			<Icon src={workHeaderIcon([step]) ?? '/icons/wrench.svg'} />
			<div class="min-w-0 flex-1 overflow-hidden text-sm text-[var(--muted)]">
				<ActivityLabel
					text={toolRowLabel(step.tool.name, step.tool.summary)}
					running={step.tool.running && !nested}
				/>
			</div>
		</div>
	{/if}
{/snippet}
