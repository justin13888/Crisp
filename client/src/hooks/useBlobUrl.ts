import { useEffect, useState } from "react";
import type { Bundle } from "../lib/crisp";

/**
 * An object URL for a blob in an open bundle, revoked when the component
 * unmounts or the id changes.
 */
export function useBlobUrl(
  bundle: Bundle | null,
  id: string | null | undefined,
): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!bundle || !id) {
      setUrl(null);
      return;
    }
    const data = bundle.blob(id);
    if (!data) {
      setUrl(null);
      return;
    }
    const next = URL.createObjectURL(
      new Blob([data.bytes.slice().buffer], { type: data.media_type }),
    );
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [bundle, id]);
  return url;
}
