const MAX_BYTES = 15 * 1024 * 1024;

export type DocumentErrorCode = 'unsupported' | 'doc' | 'empty' | 'too-large' | 'read-failed';

export class DocumentReadError extends Error {
	constructor(
		readonly code: DocumentErrorCode,
		readonly fileName: string
	) {
		super(code);
	}
}

export async function extractDocumentText(fileName: string, bytes: Uint8Array): Promise<string> {
	if (bytes.byteLength > MAX_BYTES) throw new DocumentReadError('too-large', fileName);
	const extension = fileExtension(fileName);
	if (extension === 'doc') throw new DocumentReadError('doc', fileName);
	if (
		extension !== 'md' &&
		extension !== 'markdown' &&
		extension !== 'docx' &&
		extension !== 'pdf'
	) {
		throw new DocumentReadError('unsupported', fileName);
	}
	try {
		const text =
			extension === 'docx'
				? await readDocx(bytes)
				: extension === 'pdf'
					? await readPdf(bytes)
					: new TextDecoder().decode(bytes);
		if (!text.trim()) throw new DocumentReadError('empty', fileName);
		return text;
	} catch (error) {
		if (error instanceof DocumentReadError) throw error;
		throw new DocumentReadError('read-failed', fileName);
	}
}

export function combineDocuments(
	parts: Array<{ name: string; text: string }>,
	maxChars = 24_000
): string {
	const body = parts
		.map((part) => `# ${part.name}\n${part.text.trim()}`)
		.filter((part) => part.trim())
		.join('\n\n');
	if (body.length <= maxChars) return body;
	return `${body.slice(0, maxChars)}\n\n[truncated]`;
}

async function readDocx(bytes: Uint8Array): Promise<string> {
	const mammoth = await import('mammoth');
	const arrayBuffer = new ArrayBuffer(bytes.byteLength);
	new Uint8Array(arrayBuffer).set(bytes);
	const result = await mammoth.extractRawText({ arrayBuffer });
	return result.value ?? '';
}

async function readPdf(bytes: Uint8Array): Promise<string> {
	const pdfjs = await import('pdfjs-dist');
	if (!pdfjs.GlobalWorkerOptions.workerSrc) {
		pdfjs.GlobalWorkerOptions.workerSrc = new URL(
			'pdfjs-dist/build/pdf.worker.min.mjs',
			import.meta.url
		).toString();
	}
	const document = await pdfjs.getDocument({ data: new Uint8Array(bytes) }).promise;
	const pages: string[] = [];
	for (let pageNumber = 1; pageNumber <= document.numPages; pageNumber += 1) {
		const page = await document.getPage(pageNumber);
		const content = await page.getTextContent();
		const line = content.items
			.map((item) => ('str' in item ? item.str : ''))
			.join(' ')
			.trim();
		if (line) pages.push(line);
	}
	return pages.join('\n');
}

function fileExtension(fileName: string): string {
	const match = /\.([a-z0-9]+)$/i.exec(fileName.trim());
	return match?.[1]?.toLowerCase() ?? '';
}
