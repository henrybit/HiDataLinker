export function downloadTextFile(filename: string, content: string, mime = 'application/sql') {
	downloadBlob(filename, new Blob([content], { type: `${mime};charset=utf-8` }));
}

export function downloadBlob(filename: string, blob: Blob) {
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = filename;
	anchor.rel = 'noopener';
	document.body.appendChild(anchor);
	anchor.click();
	anchor.remove();
	URL.revokeObjectURL(url);
}
