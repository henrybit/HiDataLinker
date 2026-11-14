<script lang="ts">
	import { untrack } from 'svelte';
	import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
	import { sql } from '@codemirror/lang-sql';
	import {
		bracketMatching,
		defaultHighlightStyle,
		foldGutter,
		foldKeymap,
		indentOnInput,
		syntaxHighlighting
	} from '@codemirror/language';
	import { Compartment, EditorState } from '@codemirror/state';
	import {
		drawSelection,
		EditorView,
		highlightActiveLine,
		keymap,
		placeholder,
		type ViewUpdate
	} from '@codemirror/view';
	import { sqlDialectFor } from '$lib/sql/dialect';

	let {
		value = $bindable(''),
		selection = $bindable(''),
		engine = null,
		placeholderText = '',
		active = true,
		onRun,
		onFormat,
		onExplain,
		onAssist
	}: {
		value?: string;
		/** Text of the main selection; empty when the cursor has no range. */
		selection?: string;
		engine?: string | null;
		placeholderText?: string;
		active?: boolean;
		onRun?: () => void;
		onFormat?: () => void;
		onExplain?: () => void;
		onAssist?: () => void;
	} = $props();

	let host = $state<HTMLDivElement | undefined>();
	let view = $state<EditorView | undefined>();
	const dialectCompartment = new Compartment();
	let runCallback: (() => void) | undefined;
	let formatCallback: (() => void) | undefined;
	let explainCallback: (() => void) | undefined;
	let assistCallback: (() => void) | undefined;

	$effect.pre(() => {
		runCallback = onRun;
		formatCallback = onFormat;
		explainCallback = onExplain;
		assistCallback = onAssist;
	});

	/** Replace the selection, or insert at the cursor on lines of its own, then focus. */
	export function insertText(text: string) {
		const editor = view;
		if (!editor) return;
		const range = editor.state.selection.main;
		let insert = text;
		if (range.empty) {
			const doc = editor.state.doc;
			const before = range.from > 0 ? doc.sliceString(range.from - 1, range.from) : '\n';
			const after = range.to < doc.length ? doc.sliceString(range.to, range.to + 1) : '\n';
			insert = `${before === '\n' ? '' : '\n'}${text}${after === '\n' ? '' : '\n'}`;
		}
		editor.dispatch({
			changes: { from: range.from, to: range.to, insert },
			selection: { anchor: range.from + insert.length },
			scrollIntoView: true
		});
		editor.focus();
	}

	function languageExtension(engineValue: string | null | undefined) {
		return sql({
			dialect: sqlDialectFor(engineValue),
			upperCaseKeywords: true
		});
	}

	$effect(() => {
		const parent = host;
		if (!parent) return;

		const initialDoc = untrack(() => value);
		const initialEngine = untrack(() => engine);
		const initialPlaceholder = untrack(() => placeholderText);

		const editor = new EditorView({
			parent,
			state: EditorState.create({
				doc: initialDoc,
				extensions: [
					history(),
					drawSelection(),
					indentOnInput(),
					bracketMatching(),
					foldGutter(),
					highlightActiveLine(),
					syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
					EditorView.lineWrapping,
					keymap.of([
						indentWithTab,
						...defaultKeymap,
						...historyKeymap,
						...foldKeymap,
						{
							key: 'Mod-Enter',
							run: () => {
								runCallback?.();
								return true;
							}
						},
						{
							key: 'Mod-Shift-f',
							run: () => {
								formatCallback?.();
								return true;
							}
						},
						{
							key: 'Mod-Shift-e',
							run: () => {
								explainCallback?.();
								return true;
							}
						},
						{
							key: 'Mod-k',
							run: () => {
								if (!assistCallback) return false;
								assistCallback();
								return true;
							}
						}
					]),
					dialectCompartment.of(languageExtension(initialEngine)),
					placeholder(initialPlaceholder),
					EditorView.updateListener.of((update: ViewUpdate) => {
						if (update.docChanged) {
							value = update.state.doc.toString();
						}
						if (update.docChanged || update.selectionSet) {
							const range = update.state.selection.main;
							selection = range.empty ? '' : update.state.sliceDoc(range.from, range.to);
						}
					}),
					EditorView.theme({
						'&': {
							height: '100%',
							fontSize: '12.5px'
						},
						'.cm-scroller': {
							fontFamily: 'var(--mono)',
							lineHeight: '1.55',
							overflow: 'auto'
						},
						'.cm-content': {
							padding: '10px 0',
							caretColor: 'var(--text)'
						},
						'.cm-gutters': {
							backgroundColor: 'var(--bg-muted)',
							color: 'var(--text-muted)',
							border: 'none',
							borderRight: '1px solid var(--border)'
						},
						'.cm-activeLine': {
							backgroundColor: 'rgba(212, 228, 255, 0.45)'
						},
						'.cm-activeLineGutter': {
							backgroundColor: 'rgba(212, 228, 255, 0.45)'
						},
						'&.cm-focused': {
							outline: 'none'
						}
					})
				]
			})
		});
		view = editor;

		return () => {
			editor.destroy();
			if (view === editor) view = undefined;
		};
	});

	$effect(() => {
		const editor = view;
		const nextEngine = engine;
		if (!editor) return;
		editor.dispatch({
			effects: dialectCompartment.reconfigure(languageExtension(nextEngine))
		});
	});

	$effect(() => {
		const editor = view;
		const next = value;
		if (!editor) return;
		const current = editor.state.doc.toString();
		if (next === current) return;
		editor.dispatch({
			changes: { from: 0, to: current.length, insert: next }
		});
	});

	$effect(() => {
		const editor = view;
		if (!editor || !active) return;
		const frame = requestAnimationFrame(() => {
			editor.requestMeasure();
		});
		return () => cancelAnimationFrame(frame);
	});
</script>

<div class="sql-editor" bind:this={host}></div>
