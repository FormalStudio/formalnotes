const STANDARD = /^[A-Za-z0-9+/]*={0,2}$/;

/** Standard base64 with padding, or null for anything else. */
export function decodeBase64(text: string): Uint8Array | null {
	if (text.length % 4 !== 0 || !STANDARD.test(text)) return null;
	const binary = atob(text);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
	return bytes;
}

export function encodeBase64(bytes: Uint8Array): string {
	let binary = "";
	// Spreading a whole blob into fromCharCode overflows the argument limit.
	for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	return btoa(binary);
}

/** `0x01 || nonce || ciphertext || tag`: the version byte, 12 nonce bytes and 16 tag bytes. */
export function isSealed(bytes: Uint8Array): boolean {
	return bytes.length >= 29 && bytes[0] === 1;
}
