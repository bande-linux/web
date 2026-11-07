[Exposed=Window]
interface DOMException {
  constructor(optional DOMString message = "", optional DOMString name = "Error");
  readonly attribute DOMString name;
  readonly attribute DOMString message;
  readonly attribute unsigned short code;
  const unsigned short INDEX_SIZE_ERR = 1;
  const unsigned short DOMSTRING_SIZE_ERR = 2;
  const unsigned short HIERARCHY_REQUEST_ERR = 3;
  const unsigned short WRONG_DOCUMENT_ERR = 4;
  const unsigned short INVALID_CHARACTER_ERR = 5;
  const unsigned short NO_DATA_ALLOWED_ERR = 6;
  const unsigned short NO_MODIFICATION_ALLOWED_ERR = 7;
  const unsigned short NOT_FOUND_ERR = 8;
  const unsigned short NOT_SUPPORTED_ERR = 9;
  const unsigned short INUSE_ATTRIBUTE_ERR = 10;
  const unsigned short INVALID_STATE_ERR = 11;
  const unsigned short SYNTAX_ERR = 12;
  const unsigned short INVALID_MODIFICATION_ERR = 13;
  const unsigned short NAMESPACE_ERR = 14;
  const unsigned short INVALID_ACCESS_ERR = 15;
  const unsigned short VALIDATION_ERR = 16;
  const unsigned short TYPE_MISMATCH_ERR = 17;
  const unsigned short SECURITY_ERR = 18;
  const unsigned short NETWORK_ERR = 19;
  const unsigned short ABORT_ERR = 20;
  const unsigned short URL_MISMATCH_ERR = 21;
  const unsigned short QUOTA_EXCEEDED_ERR = 22;
  const unsigned short TIMEOUT_ERR = 23;
  const unsigned short INVALID_NODE_TYPE_ERR = 24;
  const unsigned short DATA_CLONE_ERR = 25;
};

dictionary EventInit {
  boolean bubbles = false;
  boolean cancelable = false;
  boolean composed = false;
};

[Exposed=Window]
interface Event {
  constructor(DOMString type, optional EventInit eventInitDict = {});
  readonly attribute DOMString type;
  readonly attribute EventTarget? target;
  readonly attribute EventTarget? srcElement;
  readonly attribute EventTarget? currentTarget;
  sequence<EventTarget> composedPath();
  const unsigned short NONE = 0;
  const unsigned short CAPTURING_PHASE = 1;
  const unsigned short AT_TARGET = 2;
  const unsigned short BUBBLING_PHASE = 3;
  readonly attribute unsigned short eventPhase;
  undefined stopPropagation();
  attribute boolean cancelBubble;
  undefined stopImmediatePropagation();
  readonly attribute boolean bubbles;
  readonly attribute boolean cancelable;
  attribute boolean returnValue;
  undefined preventDefault();
  readonly attribute boolean defaultPrevented;
  readonly attribute boolean composed;
  readonly attribute boolean isTrusted;
  readonly attribute double timeStamp;
  undefined initEvent(DOMString type, optional boolean bubbles = false, optional boolean cancelable = false);
};

dictionary CustomEventInit : EventInit {
  any detail = null;
};

[Exposed=Window]
interface CustomEvent : Event {
  constructor(DOMString type, optional CustomEventInit eventInitDict = {});
  [EventProp] readonly attribute any detail;
  undefined initCustomEvent(DOMString type, optional boolean bubbles = false, optional boolean cancelable = false, optional any detail = null);
};

dictionary UIEventInit : EventInit {
  long detail = 0;
};

[Exposed=Window]
interface UIEvent : Event {
  constructor(DOMString type, optional UIEventInit eventInitDict = {});
  [EventProp] readonly attribute any detail;
};

dictionary MouseEventInit : UIEventInit {
  double screenX = 0;
  double screenY = 0;
  double clientX = 0;
  double clientY = 0;
  short button = 0;
  unsigned short buttons = 0;
  boolean ctrlKey = false;
  boolean shiftKey = false;
  boolean altKey = false;
  boolean metaKey = false;
  EventTarget? relatedTarget = null;
};

[Exposed=Window]
interface MouseEvent : UIEvent {
  constructor(DOMString type, optional MouseEventInit eventInitDict = {});
  [EventProp] readonly attribute any screenX;
  [EventProp] readonly attribute any screenY;
  [EventProp] readonly attribute any clientX;
  [EventProp] readonly attribute any clientY;
  [EventProp] readonly attribute any pageX;
  [EventProp] readonly attribute any pageY;
  [EventProp] readonly attribute any x;
  [EventProp] readonly attribute any y;
  [EventProp] readonly attribute any button;
  [EventProp] readonly attribute any buttons;
  [EventProp] readonly attribute any ctrlKey;
  [EventProp] readonly attribute any shiftKey;
  [EventProp] readonly attribute any altKey;
  [EventProp] readonly attribute any metaKey;
  [EventProp] readonly attribute any relatedTarget;
};

dictionary ErrorEventInit : EventInit {
  DOMString message = "";
  USVString filename = "";
  unsigned long lineno = 0;
  unsigned long colno = 0;
  any error = null;
};

[Exposed=Window]
interface ErrorEvent : Event {
  constructor(DOMString type, optional ErrorEventInit eventInitDict = {});
  [EventProp] readonly attribute any message;
  [EventProp] readonly attribute any filename;
  [EventProp] readonly attribute any lineno;
  [EventProp] readonly attribute any colno;
  [EventProp] readonly attribute any error;
};

dictionary PromiseRejectionEventInit : EventInit {
  required any promise;
  any reason;
};

[Exposed=Window]
interface PromiseRejectionEvent : Event {
  constructor(DOMString type, PromiseRejectionEventInit eventInitDict);
  [EventProp] readonly attribute any promise;
  [EventProp] readonly attribute any reason;
};

dictionary MessageEventInit : EventInit {
  any data = null;
  USVString origin = "";
  DOMString lastEventId = "";
};

[Exposed=Window]
interface MessageEvent : Event {
  constructor(DOMString type, optional MessageEventInit eventInitDict = {});
  [EventProp] readonly attribute any data;
  [EventProp] readonly attribute any origin;
  [EventProp] readonly attribute any lastEventId;
  [EventProp] readonly attribute any source;
};

callback interface EventListener {
  undefined handleEvent(Event event);
};

dictionary EventListenerOptions {
  boolean capture = false;
};

dictionary AddEventListenerOptions : EventListenerOptions {
  boolean passive;
  boolean once = false;
  AbortSignal signal;
};

[Exposed=Window]
interface EventTarget {
  constructor();
  undefined addEventListener(DOMString type, EventListener? callback, optional (AddEventListenerOptions or boolean) options = {});
  undefined removeEventListener(DOMString type, EventListener? callback, optional (EventListenerOptions or boolean) options = {});
  [Script] boolean dispatchEvent(Event event);
};

[Exposed=Window]
interface AbortController {
  constructor();
  [Script] readonly attribute AbortSignal signal;
  [Script] undefined abort(optional any reason);
};

[Exposed=Window]
interface AbortSignal : EventTarget {
  [Script] static AbortSignal abort(optional any reason);
  [Script] static AbortSignal timeout(unsigned long long milliseconds);
  [Script] static AbortSignal any(sequence<AbortSignal> signals);
  readonly attribute boolean aborted;
  readonly attribute any reason;
  [Script] undefined throwIfAborted();
  attribute EventHandler onabort;
};

[Exposed=Window]
interface NodeList {
  Node? item(unsigned long index);
  readonly attribute unsigned long length;
  iterable<Node>;
};

[Exposed=Window]
interface HTMLCollection {
  readonly attribute unsigned long length;
  Element? item(unsigned long index);
  Element? namedItem(DOMString name);
};

[Exposed=Window]
interface DOMTokenList {
  readonly attribute unsigned long length;
  DOMString? item(unsigned long index);
  boolean contains(DOMString token);
  undefined add(DOMString... tokens);
  undefined remove(DOMString... tokens);
  boolean toggle(DOMString token, optional boolean force);
  boolean replace(DOMString token, DOMString newToken);
  boolean supports(DOMString token);
  stringifier attribute DOMString value;
  iterable<DOMString>;
};

[Exposed=Window]
interface NamedNodeMap {
  readonly attribute unsigned long length;
  Attr? item(unsigned long index);
  Attr? getNamedItem(DOMString qualifiedName);
  Attr? getNamedItemNS(DOMString? namespace, DOMString localName);
  Attr? setNamedItem(Attr attr);
  Attr? setNamedItemNS(Attr attr);
  Attr removeNamedItem(DOMString qualifiedName);
  Attr removeNamedItemNS(DOMString? namespace, DOMString localName);
};

callback MutationCallback = undefined (sequence<MutationRecord> mutations, MutationObserver observer);

dictionary MutationObserverInit {
  boolean childList = false;
  boolean attributes;
  boolean characterData;
  boolean subtree = false;
  boolean attributeOldValue;
  boolean characterDataOldValue;
  sequence<DOMString> attributeFilter;
};

[Exposed=Window]
interface MutationObserver {
  constructor(MutationCallback callback);
  undefined observe(Node target, optional MutationObserverInit options = {});
  undefined disconnect();
  sequence<MutationRecord> takeRecords();
};

[Exposed=Window]
interface MutationRecord {
  readonly attribute DOMString type;
  readonly attribute Node target;
  [Script] readonly attribute NodeList addedNodes;
  [Script] readonly attribute NodeList removedNodes;
  readonly attribute Node? previousSibling;
  readonly attribute Node? nextSibling;
  readonly attribute DOMString? attributeName;
  readonly attribute DOMString? attributeNamespace;
  readonly attribute DOMString? oldValue;
};

dictionary GetRootNodeOptions {
  boolean composed = false;
};

[Exposed=Window]
interface Node : EventTarget {
  const unsigned short ELEMENT_NODE = 1;
  const unsigned short ATTRIBUTE_NODE = 2;
  const unsigned short TEXT_NODE = 3;
  const unsigned short CDATA_SECTION_NODE = 4;
  const unsigned short ENTITY_REFERENCE_NODE = 5;
  const unsigned short ENTITY_NODE = 6;
  const unsigned short PROCESSING_INSTRUCTION_NODE = 7;
  const unsigned short COMMENT_NODE = 8;
  const unsigned short DOCUMENT_NODE = 9;
  const unsigned short DOCUMENT_TYPE_NODE = 10;
  const unsigned short DOCUMENT_FRAGMENT_NODE = 11;
  const unsigned short NOTATION_NODE = 12;
  readonly attribute unsigned short nodeType;
  readonly attribute DOMString nodeName;
  readonly attribute USVString baseURI;
  readonly attribute boolean isConnected;
  readonly attribute Document? ownerDocument;
  Node getRootNode(optional GetRootNodeOptions options = {});
  readonly attribute Node? parentNode;
  readonly attribute Element? parentElement;
  boolean hasChildNodes();
  [Script] readonly attribute NodeList childNodes;
  readonly attribute Node? firstChild;
  readonly attribute Node? lastChild;
  readonly attribute Node? previousSibling;
  readonly attribute Node? nextSibling;
  [LegacyNullToEmptyString] attribute DOMString? nodeValue;
  attribute DOMString? textContent;
  undefined normalize();
  [NewObject] Node cloneNode(optional boolean subtree = false);
  boolean isEqualNode(Node? otherNode);
  boolean isSameNode(Node? otherNode);
  const unsigned short DOCUMENT_POSITION_DISCONNECTED = 0x01;
  const unsigned short DOCUMENT_POSITION_PRECEDING = 0x02;
  const unsigned short DOCUMENT_POSITION_FOLLOWING = 0x04;
  const unsigned short DOCUMENT_POSITION_CONTAINS = 0x08;
  const unsigned short DOCUMENT_POSITION_CONTAINED_BY = 0x10;
  const unsigned short DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC = 0x20;
  unsigned short compareDocumentPosition(Node other);
  boolean contains(Node? other);
  DOMString? lookupPrefix(DOMString? namespace);
  DOMString? lookupNamespaceURI(DOMString? prefix);
  boolean isDefaultNamespace(DOMString? namespace);
  Node insertBefore(Node node, Node? child);
  Node appendChild(Node node);
  Node replaceChild(Node node, Node child);
  Node removeChild(Node child);
};

interface mixin ParentNode {
  [Script] readonly attribute HTMLCollection children;
  readonly attribute Element? firstElementChild;
  readonly attribute Element? lastElementChild;
  readonly attribute unsigned long childElementCount;
  undefined prepend((Node or DOMString)... nodes);
  undefined append((Node or DOMString)... nodes);
  undefined replaceChildren((Node or DOMString)... nodes);
  Element? querySelector(DOMString selectors);
  [Script] NodeList querySelectorAll(DOMString selectors);
};

interface mixin NonDocumentTypeChildNode {
  readonly attribute Element? previousElementSibling;
  readonly attribute Element? nextElementSibling;
};

interface mixin ChildNode {
  undefined before((Node or DOMString)... nodes);
  undefined after((Node or DOMString)... nodes);
  undefined replaceWith((Node or DOMString)... nodes);
  undefined remove();
};

dictionary ElementCreationOptions {
  DOMString is;
};

[Exposed=Window]
interface Document : Node {
  constructor();
  [Script] readonly attribute DOMImplementation implementation;
  readonly attribute USVString URL;
  readonly attribute USVString documentURI;
  readonly attribute DOMString compatMode;
  readonly attribute DOMString characterSet;
  readonly attribute DOMString charset;
  readonly attribute DOMString inputEncoding;
  readonly attribute DOMString contentType;
  readonly attribute DocumentType? doctype;
  readonly attribute Element? documentElement;
  [Script] HTMLCollection getElementsByTagName(DOMString qualifiedName);
  [Script] HTMLCollection getElementsByTagNameNS(DOMString? namespace, DOMString localName);
  [Script] HTMLCollection getElementsByClassName(DOMString classNames);
  [NewObject] Element createElement(DOMString localName, optional (DOMString or ElementCreationOptions) options = {});
  [NewObject] Element createElementNS(DOMString? namespace, DOMString qualifiedName, optional (DOMString or ElementCreationOptions) options = {});
  [NewObject] DocumentFragment createDocumentFragment();
  [NewObject] Text createTextNode(DOMString data);
  [NewObject] CDATASection createCDATASection(DOMString data);
  [NewObject] Comment createComment(DOMString data);
  [NewObject] ProcessingInstruction createProcessingInstruction(DOMString target, DOMString data);
  [NewObject] Node importNode(Node node, optional boolean subtree = false);
  Node adoptNode(Node node);
  [NewObject] Attr createAttribute(DOMString localName);
  [NewObject] Attr createAttributeNS(DOMString? namespace, DOMString qualifiedName);
  [NewObject] Event createEvent(DOMString interface);
  [NewObject] Range createRange();
  [NewObject] NodeIterator createNodeIterator(Node root, optional unsigned long whatToShow = 0xFFFFFFFF, optional NodeFilter? filter = null);
  [NewObject] TreeWalker createTreeWalker(Node root, optional unsigned long whatToShow = 0xFFFFFFFF, optional NodeFilter? filter = null);
  Element? getElementById(DOMString elementId);
  attribute DOMString title;
  attribute HTMLElement? body;
  readonly attribute HTMLElement? head;
  [Script] readonly attribute HTMLCollection images;
  [Script] readonly attribute HTMLCollection links;
  [Script] readonly attribute HTMLCollection forms;
  [Script] readonly attribute HTMLCollection scripts;
  [Script] readonly attribute HTMLCollection anchors;
  [Script] readonly attribute HTMLCollection embeds;
  [Script] readonly attribute HTMLCollection plugins;
  [Script] NodeList getElementsByName(DOMString elementName);
  readonly attribute Element? currentScript;
  readonly attribute DOMString readyState;
  attribute DOMString dir;
  readonly attribute DOMString referrer;
  attribute DOMString cookie;
  readonly attribute DOMString lastModified;
  readonly attribute Window? defaultView;
  readonly attribute boolean hidden;
  readonly attribute DOMString visibilityState;
  readonly attribute Element? activeElement;
  boolean hasFocus();
  undefined write(DOMString... text);
  undefined writeln(DOMString... text);
  attribute EventHandler onreadystatechange;
  attribute EventHandler onvisibilitychange;
};

[Exposed=Window]
interface HTMLDocument : Document {
};

[Exposed=Window]
interface XMLDocument : Document {
};

[Exposed=Window]
interface DOMImplementation {
  [NewObject] DocumentType createDocumentType(DOMString name, DOMString publicId, DOMString systemId);
  [NewObject] XMLDocument createDocument(DOMString? namespace, [LegacyNullToEmptyString] DOMString qualifiedName, optional DocumentType? doctype = null);
  [NewObject] HTMLDocument createHTMLDocument(optional DOMString title);
  boolean hasFeature();
};

[Exposed=Window]
interface DocumentType : Node {
  readonly attribute DOMString name;
  readonly attribute DOMString publicId;
  readonly attribute DOMString systemId;
};

[Exposed=Window]
interface DocumentFragment : Node {
  constructor();
  Element? getElementById(DOMString elementId);
};

[Exposed=Window]
interface Element : Node {
  readonly attribute DOMString? namespaceURI;
  readonly attribute DOMString? prefix;
  readonly attribute DOMString localName;
  readonly attribute DOMString tagName;
  attribute DOMString id;
  attribute DOMString className;
  [Script, PutForwards=value] readonly attribute DOMTokenList classList;
  attribute DOMString slot;
  boolean hasAttributes();
  [Script] readonly attribute NamedNodeMap attributes;
  sequence<DOMString> getAttributeNames();
  DOMString? getAttribute(DOMString qualifiedName);
  DOMString? getAttributeNS(DOMString? namespace, DOMString localName);
  undefined setAttribute(DOMString qualifiedName, DOMString value);
  undefined setAttributeNS(DOMString? namespace, DOMString qualifiedName, DOMString value);
  undefined removeAttribute(DOMString qualifiedName);
  undefined removeAttributeNS(DOMString? namespace, DOMString localName);
  boolean toggleAttribute(DOMString qualifiedName, optional boolean force);
  boolean hasAttribute(DOMString qualifiedName);
  boolean hasAttributeNS(DOMString? namespace, DOMString localName);
  Attr? getAttributeNode(DOMString qualifiedName);
  Attr? getAttributeNodeNS(DOMString? namespace, DOMString localName);
  Attr? setAttributeNode(Attr attr);
  Attr? setAttributeNodeNS(Attr attr);
  Attr removeAttributeNode(Attr attr);
  Element? closest(DOMString selectors);
  boolean matches(DOMString selectors);
  boolean webkitMatchesSelector(DOMString selectors);
  [Script] HTMLCollection getElementsByTagName(DOMString qualifiedName);
  [Script] HTMLCollection getElementsByTagNameNS(DOMString? namespace, DOMString localName);
  [Script] HTMLCollection getElementsByClassName(DOMString classNames);
  Element? insertAdjacentElement(DOMString where, Element element);
  undefined insertAdjacentText(DOMString where, DOMString data);
  undefined insertAdjacentHTML(DOMString position, DOMString string);
  [LegacyNullToEmptyString] attribute DOMString innerHTML;
  [LegacyNullToEmptyString] attribute DOMString outerHTML;
  readonly attribute double clientWidth;
  readonly attribute double clientHeight;
};

[Exposed=Window]
interface Attr : Node {
  readonly attribute DOMString? namespaceURI;
  readonly attribute DOMString? prefix;
  readonly attribute DOMString localName;
  readonly attribute DOMString name;
  attribute DOMString value;
  readonly attribute Element? ownerElement;
  readonly attribute boolean specified;
};

[Exposed=Window]
interface CharacterData : Node {
  [LegacyNullToEmptyString] attribute DOMString data;
  readonly attribute unsigned long length;
  DOMString substringData(unsigned long offset, unsigned long count);
  undefined appendData(DOMString data);
  undefined insertData(unsigned long offset, DOMString data);
  undefined deleteData(unsigned long offset, unsigned long count);
  undefined replaceData(unsigned long offset, unsigned long count, DOMString data);
};

[Exposed=Window]
interface Text : CharacterData {
  constructor(optional DOMString data = "");
  [NewObject] Text splitText(unsigned long offset);
  readonly attribute DOMString wholeText;
};

[Exposed=Window]
interface CDATASection : Text {
};

[Exposed=Window]
interface ProcessingInstruction : CharacterData {
  readonly attribute DOMString target;
};

[Exposed=Window]
interface Comment : CharacterData {
  constructor(optional DOMString data = "");
};

[Exposed=Window]
interface AbstractRange {
  readonly attribute Node startContainer;
  readonly attribute unsigned long startOffset;
  readonly attribute Node endContainer;
  readonly attribute unsigned long endOffset;
  readonly attribute boolean collapsed;
};

dictionary StaticRangeInit {
  required Node startContainer;
  required unsigned long startOffset;
  required Node endContainer;
  required unsigned long endOffset;
};

[Exposed=Window]
interface StaticRange : AbstractRange {
  constructor(StaticRangeInit init);
};

[Exposed=Window]
interface Range : AbstractRange {
  constructor();
  readonly attribute Node commonAncestorContainer;
  undefined setStart(Node node, unsigned long offset);
  undefined setEnd(Node node, unsigned long offset);
  undefined setStartBefore(Node node);
  undefined setStartAfter(Node node);
  undefined setEndBefore(Node node);
  undefined setEndAfter(Node node);
  undefined collapse(optional boolean toStart = false);
  undefined selectNode(Node node);
  undefined selectNodeContents(Node node);
  const unsigned short START_TO_START = 0;
  const unsigned short START_TO_END = 1;
  const unsigned short END_TO_END = 2;
  const unsigned short END_TO_START = 3;
  short compareBoundaryPoints(unsigned short how, Range sourceRange);
  undefined deleteContents();
  [NewObject] DocumentFragment extractContents();
  [NewObject] DocumentFragment cloneContents();
  undefined insertNode(Node node);
  undefined surroundContents(Node newParent);
  [NewObject] Range cloneRange();
  undefined detach();
  boolean isPointInRange(Node node, unsigned long offset);
  short comparePoint(Node node, unsigned long offset);
  boolean intersectsNode(Node node);
  stringifier DOMString toString();
  [NewObject] DocumentFragment createContextualFragment(DOMString string);
};

[Exposed=Window]
interface NodeIterator {
  readonly attribute Node root;
  readonly attribute Node referenceNode;
  readonly attribute boolean pointerBeforeReferenceNode;
  readonly attribute unsigned long whatToShow;
  readonly attribute NodeFilter? filter;
  [Script] Node? nextNode();
  [Script] Node? previousNode();
  undefined detach();
};

[Exposed=Window]
interface TreeWalker {
  readonly attribute Node root;
  readonly attribute unsigned long whatToShow;
  readonly attribute NodeFilter? filter;
  attribute Node currentNode;
  [Script] Node? parentNode();
  [Script] Node? firstChild();
  [Script] Node? lastChild();
  [Script] Node? previousSibling();
  [Script] Node? nextSibling();
  [Script] Node? previousNode();
  [Script] Node? nextNode();
};

callback interface NodeFilter {
  unsigned short acceptNode(Node node);
};

[Exposed=Window, Alias=NodeFilter]
namespace NodeFilterConstants {
  const unsigned short FILTER_ACCEPT = 1;
  const unsigned short FILTER_REJECT = 2;
  const unsigned short FILTER_SKIP = 3;
  const unsigned long SHOW_ALL = 0xFFFFFFFF;
  const unsigned long SHOW_ELEMENT = 0x1;
  const unsigned long SHOW_ATTRIBUTE = 0x2;
  const unsigned long SHOW_TEXT = 0x4;
  const unsigned long SHOW_CDATA_SECTION = 0x8;
  const unsigned long SHOW_ENTITY_REFERENCE = 0x10;
  const unsigned long SHOW_ENTITY = 0x20;
  const unsigned long SHOW_PROCESSING_INSTRUCTION = 0x40;
  const unsigned long SHOW_COMMENT = 0x80;
  const unsigned long SHOW_DOCUMENT = 0x100;
  const unsigned long SHOW_DOCUMENT_TYPE = 0x200;
  const unsigned long SHOW_DOCUMENT_FRAGMENT = 0x400;
  const unsigned long SHOW_NOTATION = 0x800;
};

Document includes ParentNode;
DocumentFragment includes ParentNode;
Element includes ParentNode;
Element includes NonDocumentTypeChildNode;
CharacterData includes NonDocumentTypeChildNode;
DocumentType includes ChildNode;
Element includes ChildNode;
CharacterData includes ChildNode;
