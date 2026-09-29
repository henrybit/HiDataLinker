import { afterEach, describe, expect, it, vi } from 'vitest';
import {
	emptyCatalog,
	loadLlmCatalog,
	resolveStoredCatalog,
	readCatalog,
	removeProvider,
	saveLlmCatalog,
	upsertProvider,
	type LlmProviderProfile
} from './providers';

const openai: LlmProviderProfile = {
	id: 'oa',
	name: 'Work OpenAI',
	kind: 'openai',
	apiKey: 'sk-test',
	model: 'gpt-4o-mini',
	baseUrl: 'https://proxy.example/v1'
};

function memoryStorage() {
	const data = new Map<string, string>();
	return {
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => {
			data.set(key, value);
		},
		removeItem: (key: string) => {
			data.delete(key);
		}
	};
}

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('readCatalog', () => {
	it('returns an empty catalog when nothing is stored', () => {
		expect(readCatalog(null, null)).toEqual(emptyCatalog());
	});

	it('keeps valid providers and drops a missing selection', () => {
		const raw = JSON.stringify({
			providers: [openai, { id: '', kind: 'openai' }, { id: 'bad', kind: 'other' }],
			selectedId: 'missing'
		});
		expect(readCatalog(raw, null)).toEqual({
			providers: [openai],
			selectedId: 'oa'
		});
	});

	it('migrates a configured legacy provider', () => {
		const catalog = readCatalog(
			null,
			JSON.stringify({
				provider: 'anthropic',
				apiKey: 'sk-ant',
				model: 'claude-sonnet-4-5',
				baseUrl: ''
			}),
			() => 'legacy-1'
		);
		expect(catalog).toEqual({
			providers: [
				{
					id: 'legacy-1',
					name: 'Anthropic',
					kind: 'anthropic',
					apiKey: 'sk-ant',
					model: 'claude-sonnet-4-5',
					baseUrl: ''
				}
			],
			selectedId: 'legacy-1'
		});
	});

	it('ignores an untouched legacy record', () => {
		expect(
			readCatalog(
				null,
				JSON.stringify({
					provider: 'openai',
					apiKey: '',
					model: 'gpt-4o-mini',
					baseUrl: ''
				})
			)
		).toEqual(emptyCatalog());
	});

	it('prefers the new catalog over legacy data', () => {
		expect(
			readCatalog(JSON.stringify({ providers: [], selectedId: null }), '{"apiKey":"sk-old"}')
		).toEqual(emptyCatalog());
	});
});

describe('upsertProvider and removeProvider', () => {
	it('adds a provider and selects it when nothing is selected', () => {
		expect(upsertProvider(emptyCatalog(), openai)).toEqual({
			providers: [openai],
			selectedId: 'oa'
		});
	});

	it('replaces an existing provider without changing the selection', () => {
		const second: LlmProviderProfile = {
			id: 'an',
			name: 'Claude',
			kind: 'anthropic',
			apiKey: 'sk-ant',
			model: 'claude-sonnet-4-5',
			baseUrl: ''
		};
		const catalog = upsertProvider(upsertProvider(emptyCatalog(), openai), second);
		const renamed = { ...openai, name: 'Renamed' };
		expect(upsertProvider(catalog, renamed)).toEqual({
			providers: [renamed, second],
			selectedId: 'oa'
		});
	});

	it('selects the next provider after the selected one is removed', () => {
		const second: LlmProviderProfile = {
			id: 'an',
			name: 'Claude',
			kind: 'anthropic',
			apiKey: '',
			model: 'claude-sonnet-4-5',
			baseUrl: ''
		};
		const catalog = {
			providers: [openai, second],
			selectedId: 'oa'
		};
		expect(removeProvider(catalog, 'oa')).toEqual({
			providers: [second],
			selectedId: 'an'
		});
		expect(removeProvider({ providers: [second], selectedId: 'an' }, 'an')).toEqual(emptyCatalog());
	});
});

describe('resolveStoredCatalog', () => {
	it('keeps the file copy and ignores browser storage', () => {
		const resolved = resolveStoredCatalog(
			JSON.stringify({ providers: [openai], selectedId: 'oa' }),
			JSON.stringify({ providers: [{ ...openai, id: 'other' }], selectedId: 'other' }),
			null
		);
		expect(resolved.writeFile).toBe(false);
		expect(resolved.catalog.selectedId).toBe('oa');
	});

	it('migrates browser storage when the file is missing', () => {
		const resolved = resolveStoredCatalog(
			'',
			null,
			JSON.stringify({ provider: 'openai', apiKey: 'sk-live', model: 'gpt-4o-mini', baseUrl: '' })
		);
		expect(resolved.writeFile).toBe(true);
		expect(resolved.catalog.providers[0]?.apiKey).toBe('sk-live');
	});
});

describe('loadLlmCatalog', () => {
	it('writes a migrated catalog so the legacy record is only read once', () => {
		const storage = memoryStorage();
		storage.setItem(
			'db-gui-relationship-llm',
			JSON.stringify({
				provider: 'openai',
				apiKey: 'sk-live',
				model: 'gpt-4o-mini',
				baseUrl: 'https://proxy.example/v1'
			})
		);
		vi.stubGlobal('localStorage', storage);
		const loaded = loadLlmCatalog();
		expect(loaded.providers).toHaveLength(1);
		expect(loaded.providers[0]?.apiKey).toBe('sk-live');
		expect(storage.getItem('db-gui-llm-providers')).toContain('sk-live');

		storage.setItem('db-gui-relationship-llm', JSON.stringify({ apiKey: 'sk-replaced' }));
		expect(loadLlmCatalog().providers[0]?.apiKey).toBe('sk-live');
	});

	it('round-trips a saved catalog', () => {
		vi.stubGlobal('localStorage', memoryStorage());
		saveLlmCatalog({ providers: [openai], selectedId: 'oa' });
		expect(loadLlmCatalog()).toEqual({ providers: [openai], selectedId: 'oa' });
	});
});
