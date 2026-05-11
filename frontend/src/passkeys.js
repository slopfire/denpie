function bufferToBase64(buffer) {
    return btoa(String.fromCharCode(...new Uint8Array(buffer)))
        .replace(/\+/g, "-")
        .replace(/\//g, "_")
        .replace(/=/g, "");
}

function base64ToBuffer(base64) {
    const bin = atob(base64.replace(/-/g, "+").replace(/_/g, "/"));
    const len = bin.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) {
        bytes[i] = bin.charCodeAt(i);
    }
    return bytes.buffer;
}

export async function registerPasskey(challengeJson) {
    const challenge = JSON.parse(challengeJson);

    // Convert base64 fields to buffers
    challenge.publicKey.challenge = base64ToBuffer(challenge.publicKey.challenge);
    challenge.publicKey.user.id = base64ToBuffer(challenge.publicKey.user.id);
    if (challenge.publicKey.excludeCredentials) {
        challenge.publicKey.excludeCredentials.forEach(
            (c) => (c.id = base64ToBuffer(c.id)),
        );
    }

    challenge.publicKey.authenticatorSelection = challenge.publicKey.authenticatorSelection || {};
    challenge.publicKey.authenticatorSelection.residentKey = "required";
    challenge.publicKey.authenticatorSelection.requireResidentKey = true;

    const credential = await navigator.credentials.create({
        publicKey: challenge.publicKey,
    });

    return JSON.stringify({
        id: credential.id,
        rawId: bufferToBase64(credential.rawId),
        type: credential.type,
        response: {
            attestationObject: bufferToBase64(credential.response.attestationObject),
            clientDataJSON: bufferToBase64(credential.response.clientDataJSON),
        },
    });
}

export async function loginPasskey(challengeJson) {
    const challenge = JSON.parse(challengeJson);

    challenge.publicKey.challenge = base64ToBuffer(challenge.publicKey.challenge);
    if (challenge.publicKey.allowCredentials) {
        challenge.publicKey.allowCredentials.forEach(
            (c) => (c.id = base64ToBuffer(c.id)),
        );
    }

    const assertion = await navigator.credentials.get({
        publicKey: challenge.publicKey,
    });

    return JSON.stringify({
        id: assertion.id,
        rawId: bufferToBase64(assertion.rawId),
        type: assertion.type,
        response: {
            authenticatorData: bufferToBase64(assertion.response.authenticatorData),
            clientDataJSON: bufferToBase64(assertion.response.clientDataJSON),
            signature: bufferToBase64(assertion.response.signature),
            userHandle: assertion.response.userHandle
                ? bufferToBase64(assertion.response.userHandle)
                : null,
        },
    });
}
