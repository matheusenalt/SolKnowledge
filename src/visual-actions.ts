export { importPackageButton } from "./transfer";
import { invoke } from "@tauri-apps/api/core";
import { node, button, notify } from "./dom";
import type { EntryDetail } from "./ipc";
export function attachButton(
  detail: EntryDetail,
  done: (v: EntryDetail) => Promise<void>,
  originalBlock?: string,
): HTMLButtonElement {
  return button(
    originalBlock ? "Associar exportação de prévia" : "Adicionar anexo",
    () => {
      const file = node("input");
      file.type = "file";
      file.accept = ".png,.jpg,.jpeg,.svg,.pdf,.drawio,.pkt";
      file.onchange = () => {
        void (async () => {
          const f = file.files?.[0];
          if (!f) return;
          if (f.size > 50 * 1024 * 1024) throw Error("Máximo de 50 MiB");
          const ext = f.name.split(".").pop()?.toLowerCase();
          const types: Record<string, string> = {
            png: "image/png",
            jpg: "image/jpeg",
            jpeg: "image/jpeg",
            svg: "image/svg+xml",
            pdf: "application/pdf",
            drawio: "application/x-drawio",
            pkt: "application/octet-stream",
          };
          const media_type = types[ext ?? ""];
          if (!media_type) throw Error("Formato não permitido");
          if (
            originalBlock &&
            !media_type.startsWith("image/") &&
            media_type !== "application/pdf"
          )
            throw Error("Prévia exige PNG, JPEG, SVG ou PDF.");
          const bytes = new Uint8Array(await f.arrayBuffer());
          const sha256 = [
            ...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
          ]
            .map((v) => v.toString(16).padStart(2, "0"))
            .join("");
          const id = `asset-${crypto.randomUUID()}`;
          await invoke("put_asset", {
            input: {
              id,
              filename: f.name,
              media_type,
              sha256,
              bytes: [...bytes],
            },
          });
          const result = await invoke<EntryDetail>("save_block", {
            input: {
              entry_id: detail.id,
              expected_revision: detail.revision,
              patch: originalBlock
                ? { id: originalBlock, preview_asset_id: id }
                : {
                    id,
                    type: media_type.startsWith("image/")
                      ? "image"
                      : "attachment",
                    asset_id: id,
                    caption: f.name,
                  },
              confirm_personal: false,
            },
          });
          await done(result);
          notify("Anexo copiado para o armazenamento gerenciado.");
        })().catch((e) => notify(String(e)));
      };
      file.click();
    },
  );
}
