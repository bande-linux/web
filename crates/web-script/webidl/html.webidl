[Exposed=Window]
interface HTMLElement : Element {
  [Reflect] attribute DOMString title;
  [Reflect] attribute DOMString lang;
  [Reflect] attribute DOMString dir;
  [Reflect] attribute boolean hidden;
  [Reflect] attribute DOMString accessKey;
  [Reflect] attribute long tabIndex;
  attribute DOMString innerText;
  [Script] undefined click();
  undefined focus();
  undefined blur();
  [Script] readonly attribute DOMStringMap dataset;
  readonly attribute double offsetWidth;
  readonly attribute double offsetHeight;
  readonly attribute double offsetTop;
  readonly attribute double offsetLeft;
  readonly attribute Element? offsetParent;
};

HTMLElement includes GlobalEventHandlers;

[Exposed=Window]
interface HTMLUnknownElement : HTMLElement {
};

[Exposed=Window]
interface DOMStringMap {
};

[Exposed=Window, Tag=a]
interface HTMLAnchorElement : HTMLElement {
  [Reflect] attribute DOMString target;
  [Reflect] attribute DOMString download;
  [Reflect] attribute DOMString rel;
  [Reflect] attribute DOMString hreflang;
  [Reflect] attribute DOMString type;
  [Reflect] attribute USVString href;
  [Reflect] attribute DOMString name;
  attribute DOMString text;
  [Script] readonly attribute DOMTokenList relList;
};

[Exposed=Window, Tag=area]
interface HTMLAreaElement : HTMLElement {
  [Reflect] attribute DOMString alt;
  [Reflect] attribute DOMString coords;
  [Reflect] attribute DOMString shape;
  [Reflect] attribute DOMString target;
  [Reflect] attribute DOMString rel;
  [Reflect] attribute USVString href;
};

[Exposed=Window, Tag=audio]
interface HTMLAudioElement : HTMLElement {
};

[Exposed=Window, Tag=br]
interface HTMLBRElement : HTMLElement {
};

[Exposed=Window, Tag=base]
interface HTMLBaseElement : HTMLElement {
  [Reflect] attribute DOMString target;
};

[Exposed=Window, Tag=body]
interface HTMLBodyElement : HTMLElement {
  [Reflect] attribute DOMString text;
  [Reflect] attribute DOMString link;
  [Reflect] attribute DOMString vlink;
  [Reflect] attribute DOMString alink;
  [Reflect] attribute DOMString bgcolor;
};

[Exposed=Window, Tag=button]
interface HTMLButtonElement : HTMLElement {
  [Reflect] attribute boolean disabled;
  [Reflect] attribute DOMString name;
  [Reflect] attribute DOMString type;
  [Reflect] attribute DOMString value;
};

[Exposed=Window, Tag=canvas]
interface HTMLCanvasElement : HTMLElement {
  [Reflect] attribute unsigned long width;
  [Reflect] attribute unsigned long height;
};

[Exposed=Window, Tag=dl]
interface HTMLDListElement : HTMLElement {
};

[Exposed=Window, Tag=data]
interface HTMLDataElement : HTMLElement {
  [Reflect] attribute DOMString value;
};

[Exposed=Window, Tag=datalist]
interface HTMLDataListElement : HTMLElement {
};

[Exposed=Window, Tag=details]
interface HTMLDetailsElement : HTMLElement {
  [Reflect] attribute boolean open;
};

[Exposed=Window, Tag=dialog]
interface HTMLDialogElement : HTMLElement {
  [Reflect] attribute boolean open;
};

[Exposed=Window, Tag=div]
interface HTMLDivElement : HTMLElement {
  [Reflect] attribute DOMString align;
};

[Exposed=Window, Tag=embed]
interface HTMLEmbedElement : HTMLElement {
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString type;
  [Reflect] attribute DOMString width;
  [Reflect] attribute DOMString height;
};

[Exposed=Window, Tag=fieldset]
interface HTMLFieldSetElement : HTMLElement {
  [Reflect] attribute boolean disabled;
  [Reflect] attribute DOMString name;
};

[Exposed=Window, Tag=form]
interface HTMLFormElement : HTMLElement {
  [Reflect] attribute DOMString acceptCharset;
  [Reflect] attribute USVString action;
  [Reflect] attribute DOMString autocomplete;
  [Reflect] attribute DOMString enctype;
  [Reflect] attribute DOMString method;
  [Reflect] attribute DOMString name;
  [Reflect] attribute boolean noValidate;
  [Reflect] attribute DOMString target;
  [Script] undefined submit();
  [Script] undefined reset();
};

[Exposed=Window, Tag=hr]
interface HTMLHRElement : HTMLElement {
};

[Exposed=Window, Tag=head]
interface HTMLHeadElement : HTMLElement {
};

[Exposed=Window, Tag=h1, Tag=h2, Tag=h3, Tag=h4, Tag=h5, Tag=h6]
interface HTMLHeadingElement : HTMLElement {
  [Reflect] attribute DOMString align;
};

[Exposed=Window, Tag=html]
interface HTMLHtmlElement : HTMLElement {
  [Reflect] attribute DOMString version;
};

[Exposed=Window, Tag=iframe]
interface HTMLIFrameElement : HTMLElement {
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString srcdoc;
  [Reflect] attribute DOMString name;
  [Reflect] attribute DOMString width;
  [Reflect] attribute DOMString height;
  [Reflect] attribute DOMString allow;
  [Reflect] attribute DOMString loading;
};

[Exposed=Window, Tag=img]
interface HTMLImageElement : HTMLElement {
  [Reflect] attribute DOMString alt;
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString srcset;
  [Reflect] attribute DOMString sizes;
  [Reflect] attribute DOMString? crossOrigin;
  [Reflect] attribute DOMString useMap;
  [Reflect] attribute boolean isMap;
  [Reflect] attribute unsigned long width;
  [Reflect] attribute unsigned long height;
  [Reflect] attribute DOMString decoding;
  [Reflect] attribute DOMString loading;
  readonly attribute unsigned long naturalWidth;
  readonly attribute unsigned long naturalHeight;
  readonly attribute boolean complete;
};

[Exposed=Window, Tag=input]
interface HTMLInputElement : HTMLElement {
  [Reflect] attribute DOMString accept;
  [Reflect] attribute DOMString alt;
  [Reflect] attribute DOMString autocomplete;
  [Reflect=checked] attribute boolean defaultChecked;
  [Reflect] attribute DOMString dirName;
  [Reflect] attribute boolean disabled;
  [Reflect] attribute DOMString max;
  [Reflect=maxlength] attribute long maxLength;
  [Reflect] attribute DOMString min;
  [Reflect=minlength] attribute long minLength;
  [Reflect] attribute boolean multiple;
  [Reflect] attribute DOMString name;
  [Reflect] attribute DOMString pattern;
  [Reflect] attribute DOMString placeholder;
  [Reflect] attribute boolean readOnly;
  [Reflect] attribute boolean required;
  [Reflect] attribute unsigned long size;
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString step;
  [Reflect] attribute DOMString type;
  [Reflect=value] attribute DOMString defaultValue;
  attribute DOMString value;
  attribute boolean checked;
};

[Exposed=Window, Tag=li]
interface HTMLLIElement : HTMLElement {
  [Reflect] attribute long value;
  [Reflect] attribute DOMString type;
};

[Exposed=Window, Tag=label]
interface HTMLLabelElement : HTMLElement {
  [Reflect=for] attribute DOMString htmlFor;
};

[Exposed=Window, Tag=legend]
interface HTMLLegendElement : HTMLElement {
};

[Exposed=Window, Tag=link]
interface HTMLLinkElement : HTMLElement {
  [Reflect] attribute USVString href;
  [Reflect] attribute DOMString? crossOrigin;
  [Reflect] attribute DOMString rel;
  [Reflect] attribute DOMString as;
  [Reflect] attribute DOMString media;
  [Reflect] attribute DOMString hreflang;
  [Reflect] attribute DOMString type;
  [Reflect] attribute DOMString sizes;
  [Reflect] attribute boolean disabled;
  [Script] readonly attribute DOMTokenList relList;
};

[Exposed=Window, Tag=map]
interface HTMLMapElement : HTMLElement {
  [Reflect] attribute DOMString name;
};

[Exposed=Window, Tag=menu]
interface HTMLMenuElement : HTMLElement {
};

[Exposed=Window, Tag=meta]
interface HTMLMetaElement : HTMLElement {
  [Reflect] attribute DOMString name;
  [Reflect=http-equiv] attribute DOMString httpEquiv;
  [Reflect] attribute DOMString content;
  [Reflect] attribute DOMString media;
};

[Exposed=Window, Tag=meter]
interface HTMLMeterElement : HTMLElement {
};

[Exposed=Window, Tag=ins, Tag=del]
interface HTMLModElement : HTMLElement {
  [Reflect] attribute USVString cite;
  [Reflect=datetime] attribute DOMString dateTime;
};

[Exposed=Window, Tag=ol]
interface HTMLOListElement : HTMLElement {
  [Reflect] attribute boolean reversed;
  [Reflect] attribute long start;
  [Reflect] attribute DOMString type;
};

[Exposed=Window, Tag=object]
interface HTMLObjectElement : HTMLElement {
  [Reflect] attribute USVString data;
  [Reflect] attribute DOMString type;
  [Reflect] attribute DOMString name;
  [Reflect] attribute DOMString width;
  [Reflect] attribute DOMString height;
};

[Exposed=Window, Tag=optgroup]
interface HTMLOptGroupElement : HTMLElement {
  [Reflect] attribute boolean disabled;
  [Reflect] attribute DOMString label;
};

[Exposed=Window, Tag=option]
interface HTMLOptionElement : HTMLElement {
  [Reflect] attribute boolean disabled;
  [Reflect] attribute DOMString label;
  [Reflect=selected] attribute boolean defaultSelected;
  [Reflect] attribute DOMString value;
};

[Exposed=Window, Tag=output]
interface HTMLOutputElement : HTMLElement {
  [Reflect] attribute DOMString name;
};

[Exposed=Window, Tag=p]
interface HTMLParagraphElement : HTMLElement {
  [Reflect] attribute DOMString align;
};

[Exposed=Window, Tag=picture]
interface HTMLPictureElement : HTMLElement {
};

[Exposed=Window, Tag=pre, Tag=listing, Tag=xmp]
interface HTMLPreElement : HTMLElement {
};

[Exposed=Window, Tag=progress]
interface HTMLProgressElement : HTMLElement {
};

[Exposed=Window, Tag=blockquote, Tag=q]
interface HTMLQuoteElement : HTMLElement {
  [Reflect] attribute USVString cite;
};

[Exposed=Window, Tag=script]
interface HTMLScriptElement : HTMLElement {
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString type;
  [Reflect=nomodule] attribute boolean noModule;
  [Reflect] attribute boolean defer;
  [Reflect] attribute DOMString? crossOrigin;
  [Reflect] attribute DOMString integrity;
  [Reflect=referrerpolicy] attribute DOMString referrerPolicy;
  [Reflect] attribute DOMString charset;
  attribute DOMString text;
  attribute boolean async;
};

[Exposed=Window, Tag=select]
interface HTMLSelectElement : HTMLElement {
  [Reflect] attribute boolean disabled;
  [Reflect] attribute boolean multiple;
  [Reflect] attribute DOMString name;
  [Reflect] attribute boolean required;
  [Reflect] attribute unsigned long size;
};

[Exposed=Window, Tag=slot]
interface HTMLSlotElement : HTMLElement {
  [Reflect] attribute DOMString name;
};

[Exposed=Window, Tag=source]
interface HTMLSourceElement : HTMLElement {
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString type;
  [Reflect] attribute DOMString srcset;
  [Reflect] attribute DOMString sizes;
  [Reflect] attribute DOMString media;
};

[Exposed=Window, Tag=span]
interface HTMLSpanElement : HTMLElement {
};

[Exposed=Window, Tag=style]
interface HTMLStyleElement : HTMLElement {
  [Reflect] attribute DOMString media;
  [Reflect] attribute DOMString type;
};

[Exposed=Window, Tag=caption]
interface HTMLTableCaptionElement : HTMLElement {
};

[Exposed=Window, Tag=td, Tag=th]
interface HTMLTableCellElement : HTMLElement {
  [Reflect=colspan] attribute unsigned long colSpan;
  [Reflect=rowspan] attribute unsigned long rowSpan;
  [Reflect] attribute DOMString headers;
  [Reflect] attribute DOMString scope;
  [Reflect] attribute DOMString abbr;
};

[Exposed=Window, Tag=col, Tag=colgroup]
interface HTMLTableColElement : HTMLElement {
  [Reflect] attribute unsigned long span;
};

[Exposed=Window, Tag=table]
interface HTMLTableElement : HTMLElement {
};

[Exposed=Window, Tag=tr]
interface HTMLTableRowElement : HTMLElement {
};

[Exposed=Window, Tag=tbody, Tag=thead, Tag=tfoot]
interface HTMLTableSectionElement : HTMLElement {
};

[Exposed=Window, Tag=template]
interface HTMLTemplateElement : HTMLElement {
  readonly attribute DocumentFragment content;
};

[Exposed=Window, Tag=textarea]
interface HTMLTextAreaElement : HTMLElement {
  [Reflect] attribute DOMString autocomplete;
  [Reflect] attribute unsigned long cols;
  [Reflect] attribute DOMString dirName;
  [Reflect] attribute boolean disabled;
  [Reflect=maxlength] attribute long maxLength;
  [Reflect=minlength] attribute long minLength;
  [Reflect] attribute DOMString name;
  [Reflect] attribute DOMString placeholder;
  [Reflect] attribute boolean readOnly;
  [Reflect] attribute boolean required;
  [Reflect] attribute unsigned long rows;
  [Reflect] attribute DOMString wrap;
  attribute DOMString value;
  attribute DOMString defaultValue;
};

[Exposed=Window, Tag=time]
interface HTMLTimeElement : HTMLElement {
  [Reflect=datetime] attribute DOMString dateTime;
};

[Exposed=Window, Tag=title]
interface HTMLTitleElement : HTMLElement {
  attribute DOMString text;
};

[Exposed=Window, Tag=track]
interface HTMLTrackElement : HTMLElement {
  [Reflect] attribute DOMString kind;
  [Reflect] attribute USVString src;
  [Reflect] attribute DOMString srclang;
  [Reflect] attribute DOMString label;
  [Reflect] attribute boolean default;
};

[Exposed=Window, Tag=ul]
interface HTMLUListElement : HTMLElement {
};

[Exposed=Window, Tag=video]
interface HTMLVideoElement : HTMLElement {
  [Reflect] attribute unsigned long width;
  [Reflect] attribute unsigned long height;
  [Reflect] attribute USVString poster;
};
