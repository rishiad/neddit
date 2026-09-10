document.addEventListener("change", (event) => {
  const control = event.target instanceof Element
    ? event.target.closest("[data-submit-on-change]")
    : null;
  if (control) {
    control.form.requestSubmit();
  }
});
