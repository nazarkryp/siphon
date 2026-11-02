import { useId } from "react";
import { Button } from "../../components/Button";
import { ClipboardIcon } from "../../components/icons";
import { describeLink } from "../../lib/links";
import { useDialogs } from "../../state/dialogs";
import { useStore } from "../../state/store";
import "./clipboard.css";

export function ClipboardPrompt() {
  const { state, dispatch } = useStore();
  const dialogs = useDialogs();
  const titleId = useId();
  const url = state.clipboardUrl;
  if (!url) return null;

  function hide() {
    dispatch({ type: "clipboardLinkChanged", url: null });
  }

  return (
    <section className="clipboard-prompt" role="dialog" aria-labelledby={titleId}>
      <div className="clipboard-prompt__content">
        <div className="clipboard-prompt__icon">
          <ClipboardIcon />
        </div>
        <div className="clipboard-prompt__text">
          <h2 className="clipboard-prompt__title" id={titleId}>
            Add this link to Siphon?
          </h2>
          <div className="clipboard-prompt__url selectable" title={url}>
            {url}
          </div>
          <div className="clipboard-prompt__meta">Copied just now · {describeLink(url)}</div>
        </div>
      </div>
      <div className="clipboard-prompt__actions">
        <Button onClick={hide}>Ignore</Button>
        <Button
          variant="primary"
          onClick={() => {
            hide();
            dialogs.open({ type: "addLink", url });
          }}
        >
          Add
        </Button>
      </div>
    </section>
  );
}
