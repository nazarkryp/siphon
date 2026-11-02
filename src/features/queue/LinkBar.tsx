import { useState, type ClipboardEvent, type FormEvent } from "react";
import { api } from "../../api";
import { Button } from "../../components/Button";
import { IconButton } from "../../components/IconButton";
import { AlertCircleIcon, CheckCircleIcon, CloseIcon, LinkIcon, SettingsIcon } from "../../components/icons";
import { errorMessage } from "../../lib/errors";
import { extractUrls } from "../../lib/links";
import { useDialogs } from "../../state/dialogs";
import { useStore } from "../../state/store";

interface Message {
  tone: "error" | "info";
  text: string;
  details?: string[];
}

export function LinkBar() {
  const { dispatch } = useStore();
  const dialogs = useDialogs();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<Message | null>(null);

  // A single-line input drops line breaks. Keep pasted lines apart with spaces.
  function onPaste(event: ClipboardEvent<HTMLInputElement>) {
    const text = event.clipboardData.getData("text");
    if (!/[\r\n]/.test(text)) return;
    event.preventDefault();
    const input = event.currentTarget;
    const start = input.selectionStart ?? value.length;
    const end = input.selectionEnd ?? value.length;
    const pasted = text.split(/\s+/).filter(Boolean).join(" ");
    setValue(value.slice(0, start) + pasted + value.slice(end));
  }

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    const urls = extractUrls(value);
    if (urls.length === 0) {
      setMessage({ tone: "error", text: "Paste a link that starts with http:// or https://." });
      return;
    }
    setMessage(null);
    if (urls.length === 1) {
      dialogs.open({ type: "addLink", url: urls[0], onAdded: () => setValue("") });
      return;
    }
    setBusy(true);
    try {
      const result = await api.addLinks(urls);
      for (const download of result.added) dispatch({ type: "downloadUpserted", download });
      if (result.failed.length > 0) {
        setValue(result.failed.map((f) => f.url).join(" "));
        setMessage({
          tone: "error",
          text: `Added ${result.added.length} of ${urls.length} links. These links failed and stay in the field:`,
          details: result.failed.map((f) => `${f.url} — ${f.message}`),
        });
      } else {
        setValue("");
        setMessage({ tone: "info", text: `Added ${result.added.length} links to the queue.` });
      }
    } catch (error) {
      setMessage({ tone: "error", text: errorMessage(error) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="link-bar">
      <form className="link-bar__row" onSubmit={onSubmit}>
        <label className="link-field">
          <LinkIcon />
          <span className="sr-only">Links</span>
          <input
            type="text"
            className="link-field__input"
            placeholder="Paste links — one per line"
            value={value}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => {
              setValue(event.target.value);
              if (message?.tone === "info") setMessage(null);
            }}
            onPaste={onPaste}
          />
        </label>
        <Button type="submit" variant="primary" loading={busy} disabled={value.trim() === ""}>
          Fetch
        </Button>
        <IconButton
          size="lg"
          label="Settings"
          icon={<SettingsIcon />}
          onClick={() => dialogs.open({ type: "settings" })}
        />
      </form>
      {message && (
        <div
          className={`link-bar__message link-bar__message--${message.tone}`}
          role={message.tone === "error" ? "alert" : "status"}
        >
          {message.tone === "error" ? <AlertCircleIcon size="sm" /> : <CheckCircleIcon size="sm" />}
          <div className="link-bar__message-text selectable">
            {message.text}
            {message.details && (
              <ul>
                {message.details.map((line) => (
                  <li key={line}>{line}</li>
                ))}
              </ul>
            )}
          </div>
          <IconButton label="Dismiss" size="sm" icon={<CloseIcon />} onClick={() => setMessage(null)} />
        </div>
      )}
    </div>
  );
}
