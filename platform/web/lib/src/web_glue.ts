export async function fetchAsset(path: string): Promise<Uint8Array> {
    const url = new URL("./enginedata/" + path, location.origin)
    console.trace("Fetching: " + url);
    const response = await fetch(url);
    if (response.status != 200) {
        throw response.status;
    }
    const buffer = await response.arrayBuffer();
    return new Uint8Array(buffer);
}
