

typedef (Function or DOMString) TimerHandler;

callback Function = any (any... arguments);
callback FrameRequestCallback = undefined (double time);
callback VoidFunction = undefined ();

interface mixin GlobalEventHandlers {
  attribute EventHandler onabort;
  attribute EventHandler onblur;
  attribute EventHandler onchange;
  attribute EventHandler onclick;
  attribute EventHandler oncontextmenu;
  attribute EventHandler ondblclick;
  attribute EventHandler onerror;
  attribute EventHandler onfocus;
  attribute EventHandler oninput;
  attribute EventHandler onkeydown;
  attribute EventHandler onkeypress;
  attribute EventHandler onkeyup;
  attribute EventHandler onload;
  attribute EventHandler onmousedown;
  attribute EventHandler onmouseenter;
  attribute EventHandler onmouseleave;
  attribute EventHandler onmousemove;
  attribute EventHandler onmouseout;
  attribute EventHandler onmouseover;
  attribute EventHandler onmouseup;
  attribute EventHandler onreset;
  attribute EventHandler onresize;
  attribute EventHandler onscroll;
  attribute EventHandler onsubmit;
  attribute EventHandler onwheel;
};

[Global=Window, Exposed=Window]
interface Window : EventTarget {
  readonly attribute Window window;
  readonly attribute Window self;
  readonly attribute Window frames;
  readonly attribute Window? parent;
  readonly attribute Window? top;
  readonly attribute any opener;
  readonly attribute unsigned long length;
  readonly attribute boolean closed;
  attribute DOMString name;
  readonly attribute Document document;
  readonly attribute Location location;
  readonly attribute History history;
  readonly attribute Navigator navigator;
  readonly attribute Performance performance;
  readonly attribute any event;
  readonly attribute double innerWidth;
  readonly attribute double innerHeight;
  readonly attribute double outerWidth;
  readonly attribute double outerHeight;
  readonly attribute double scrollX;
  readonly attribute double scrollY;
  readonly attribute double pageXOffset;
  readonly attribute double pageYOffset;
  readonly attribute double devicePixelRatio;
  long setTimeout(TimerHandler handler, optional long timeout = 0, any... arguments);
  undefined clearTimeout(optional long id = 0);
  long setInterval(TimerHandler handler, optional long timeout = 0, any... arguments);
  undefined clearInterval(optional long id = 0);
  [Script] undefined queueMicrotask(VoidFunction callback);
  unsigned long requestAnimationFrame(FrameRequestCallback callback);
  undefined cancelAnimationFrame(unsigned long handle);
  [Script] undefined reportError(any e);
  undefined postMessage(any message, optional any targetOrigin, optional any transfer);
  DOMString btoa(DOMString data);
  DOMString atob(DOMString data);
  undefined alert(optional any message);
  boolean confirm(optional any message);
  DOMString? prompt(optional any message, optional any defaultValue);
  undefined stop();
  undefined focus();
  undefined blur();
  undefined close();
  undefined scrollTo(optional any x, optional any y);
  undefined scrollBy(optional any x, optional any y);
  attribute EventHandler onbeforeunload;
  attribute EventHandler onhashchange;
  attribute EventHandler onmessage;
  attribute EventHandler onpagehide;
  attribute EventHandler onpageshow;
  attribute EventHandler onpopstate;
  attribute EventHandler onunhandledrejection;
  attribute EventHandler onrejectionhandled;
};

Window includes GlobalEventHandlers;
Document includes GlobalEventHandlers;

[Exposed=Window]
interface Location {
  stringifier attribute USVString href;
  readonly attribute USVString origin;
  attribute USVString protocol;
  attribute USVString host;
  attribute USVString hostname;
  attribute USVString port;
  attribute USVString pathname;
  attribute USVString search;
  attribute USVString hash;
  undefined assign(USVString url);
  undefined replace(USVString url);
  undefined reload();
};

[Exposed=Window]
interface History {
  readonly attribute unsigned long length;
  readonly attribute any state;
  undefined go(optional long delta = 0);
  undefined back();
  undefined forward();
  undefined pushState(any data, DOMString unused, optional USVString? url = null);
  undefined replaceState(any data, DOMString unused, optional USVString? url = null);
};

[Exposed=Window]
interface Navigator {
  readonly attribute DOMString userAgent;
  readonly attribute DOMString appCodeName;
  readonly attribute DOMString appName;
  readonly attribute DOMString appVersion;
  readonly attribute DOMString platform;
  readonly attribute DOMString product;
  readonly attribute DOMString productSub;
  readonly attribute DOMString vendor;
  readonly attribute DOMString vendorSub;
  readonly attribute DOMString language;
  readonly attribute sequence<DOMString> languages;
  readonly attribute boolean onLine;
  readonly attribute boolean cookieEnabled;
  readonly attribute unsigned long hardwareConcurrency;
  readonly attribute boolean webdriver;
};

[Exposed=Window]
interface Performance : EventTarget {
  double now();
  readonly attribute double timeOrigin;
};

[Exposed=Window]
namespace console {
  undefined log(any... data);
  undefined info(any... data);
  undefined warn(any... data);
  undefined error(any... data);
  undefined debug(any... data);
};
