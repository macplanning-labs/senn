/**
 * Base64url encoding/decoding utilities for WebAuthn
 *
 * WebAuthn APIs work with ArrayBuffers, but JSON responses from the server
 * contain challenge/credential data as base64url strings.
 * This module handles conversion between the two.
 */

/**
 * Convert a base64url-encoded string to an ArrayBuffer
 */
export function base64urlToArrayBuffer(str: string): ArrayBuffer {
  const paddingLength = (4 - (str.length % 4)) % 4;
  const padding = '='.repeat(paddingLength);
  const base64 = (str + padding)
    .replace(/-/g, '+')
    .replace(/_/g, '/');

  const binaryString = atob(base64);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i) as number;
  }
  return bytes.buffer;
}

/**
 * Convert an ArrayBuffer to a base64url-encoded string
 */
export function arrayBufferToBase64url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i] as number);
  }
  return btoa(binary)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

/**
 * Convert PublicKeyCredentialCreationOptions from server format to browser format
 * The server returns JSON with base64url-encoded challenge and credential IDs,
 * but the browser API expects ArrayBuffer for these fields.
 */
export function convertCreateOptionsToJSON(
  options: any
): any {
  return {
    ...options,
    challenge: base64urlToArrayBuffer(options.challenge),
    user: {
      ...options.user,
      id: base64urlToArrayBuffer(options.user.id),
    },
    excludeCredentials: (options.excludeCredentials || []).map((cred: any) => ({
      ...cred,
      id: base64urlToArrayBuffer(cred.id),
    })),
  };
}

/**
 * Convert a PublicKeyCredential from browser format to JSON format
 * The browser API returns ArrayBuffer in certain fields,
 * but the server expects base64url-encoded JSON.
 */
export function convertCredentialToJSON(credential: any): any {
  return {
    id: credential.id,
    rawId: arrayBufferToBase64url(credential.rawId),
    response: {
      clientDataJSON: arrayBufferToBase64url(credential.response.clientDataJSON),
      attestationObject: arrayBufferToBase64url(credential.response.attestationObject),
    },
    type: credential.type,
  };
}
