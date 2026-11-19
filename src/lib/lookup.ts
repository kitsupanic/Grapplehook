/** The link a download fetches: the one the shown info came from, and only while the URL box still holds it. */
export const downloadUrl = (box: string, fetched: string) => (fetched && box.trim() === fetched ? fetched : null);
