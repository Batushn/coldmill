import { listen } from "./event";

/** The app subscribes to drag-and-drop through the webview handle; in the
 *  preview the scene script fires those events instead of a real mouse. */
export function getCurrentWebview() {
  return {
    onDragDropEvent: (handler: (event: { payload: { type: string; paths?: string[] } }) => void) =>
      listen("preview:dragdrop", handler as never),
  };
}
