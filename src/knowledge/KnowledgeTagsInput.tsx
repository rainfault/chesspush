import { useEffect, useId, useRef, useState } from "react";
import { errorMessage } from "../api";
import { tagSuggestions } from "./api";

interface Props {
  value: string[];
  onChange: (tags: string[]) => void;
  disabled?: boolean;
  label?: string;
}

export function KnowledgeTagsInput({
  value,
  onChange,
  disabled = false,
  label = "Тег знания",
}: Props) {
  const [query, setQuery] = useState("");
  const [focused, setFocused] = useState(false);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [selected, setSelected] = useState(-1);
  const [error, setError] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const cache = useRef(new Map<string, string[]>());
  const listId = useId();
  const options = suggestions.filter(
    (tag) =>
      !value.some(
        (existing) => existing.toLocaleLowerCase() === tag.toLocaleLowerCase(),
      ),
  );
  const activeIndex = Math.min(selected, options.length - 1);
  const open = focused && !disabled && options.length > 0;

  useEffect(() => {
    if (!focused || disabled) return;
    const key = query.trim();
    const known = cache.current.get(key);
    if (known) {
      setSuggestions(known);
      setError("");
      return;
    }
    let disposed = false;
    void tagSuggestions(key)
      .then((tags) => {
        if (disposed) return;
        cache.current.set(key, tags);
        setSuggestions(tags);
        setError("");
      })
      .catch((reason) => {
        if (!disposed) {
          setSuggestions([]);
          setError(errorMessage(reason));
        }
      });
    return () => {
      disposed = true;
    };
  }, [query, focused, disabled]);

  function add(tag = query) {
    const next = tag.trim();
    if (!next || disabled) return;
    if (
      !value.some(
        (existing) => existing.toLocaleLowerCase() === next.toLocaleLowerCase(),
      )
    )
      onChange([...value, next]);
    cache.current.clear();
    setQuery("");
    setSuggestions([]);
    setSelected(-1);
    setFocused(true);
    input.current?.focus();
  }

  return (
    <div className="knowledge-tags-input">
      <span className="knowledge-tags-label">Теги</span>
      {value.length > 0 && (
        <ul
          className="knowledge-tag-chips"
          aria-label={label === "Тег партии" ? "Теги партии" : "Теги знания"}
        >
          {value.map((tag) => (
            <li key={tag}>
              <span>{tag}</span>
              <button
                type="button"
                disabled={disabled}
                aria-label={`Удалить тег «${tag}»`}
                onClick={() =>
                  onChange(value.filter((existing) => existing !== tag))
                }
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="knowledge-tag-entry">
        <input
          ref={input}
          role="combobox"
          aria-label={label}
          aria-autocomplete="list"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-activedescendant={
            open && activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined
          }
          autoComplete="off"
          value={query}
          disabled={disabled}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onChange={(event) => {
            setQuery(event.target.value);
            setSuggestions([]);
            setError("");
            setSelected(-1);
            setFocused(true);
          }}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setFocused(true);
              setSelected((index) =>
                event.key === "ArrowDown"
                  ? Math.min(index + 1, options.length - 1)
                  : Math.max(index - 1, -1),
              );
            } else if (event.key === "Enter" || event.key === ",") {
              event.preventDefault();
              add(open && activeIndex >= 0 ? options[activeIndex] : query);
            } else if (event.key === "Escape") {
              setFocused(false);
              setSelected(-1);
            } else if (event.key === "Backspace" && !query && value.length) {
              onChange(value.slice(0, -1));
            }
          }}
        />
        <button
          type="button"
          disabled={disabled || !query.trim()}
          aria-label="Добавить тег"
          onClick={() => add()}
        >
          +
        </button>
        {open && (
          <ul
            id={listId}
            className="knowledge-tag-suggestions"
            role="listbox"
            aria-label="Использованные теги"
          >
            {options.map((tag, index) => (
              <li
                id={`${listId}-${index}`}
                key={tag}
                role="option"
                aria-selected={index === activeIndex}
                onMouseDown={(event) => event.preventDefault()}
                onMouseEnter={() => setSelected(index)}
                onClick={() => add(tag)}
              >
                {tag}
              </li>
            ))}
          </ul>
        )}
      </div>
      {error && (
        <div role="alert" className="error">
          {error}
        </div>
      )}
    </div>
  );
}
