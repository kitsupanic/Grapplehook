import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link';
import { parse, type SentLink } from './sentLink';

// The newest sent link (ADR 0003) not looked up yet. It waits here while the URL box is locked:
// a link that started the app comes before the Tools check. Read once per launch, at import.
let held = $state<SentLink | null>(null);

const receive = (links: string[] | null) => {
  for (const link of links ?? []) held = parse(link) ?? held;
};
getCurrent().then(receive, () => {});
onOpenUrl(receive).catch(() => {});

export const heldLink = {
  /** The held link, handed out once and only while the box is unlocked. Reactive: an `$effect` reruns when one arrives. */
  take(locked: boolean) {
    if (locked || !held) return null;
    const link = held;
    held = null;
    return link;
  },
};
