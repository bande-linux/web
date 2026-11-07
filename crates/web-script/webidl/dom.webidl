[Exposed=Window]
interface EventTarget {
};

[Exposed=Window]
interface Node : EventTarget {
  const unsigned short ELEMENT_NODE = 1;
  const unsigned short ATTRIBUTE_NODE = 2;
  const unsigned short TEXT_NODE = 3;
  const unsigned short CDATA_SECTION_NODE = 4;
  const unsigned short PROCESSING_INSTRUCTION_NODE = 7;
  const unsigned short COMMENT_NODE = 8;
  const unsigned short DOCUMENT_NODE = 9;
  const unsigned short DOCUMENT_TYPE_NODE = 10;
  const unsigned short DOCUMENT_FRAGMENT_NODE = 11;
  readonly attribute unsigned short nodeType;
  readonly attribute DOMString nodeName;
  readonly attribute boolean isConnected;
  readonly attribute Document? ownerDocument;
  readonly attribute Node? parentNode;
  readonly attribute Element? parentElement;
  boolean hasChildNodes();
  readonly attribute sequence<Node> childNodes;
  readonly attribute Node? firstChild;
  readonly attribute Node? lastChild;
  readonly attribute Node? previousSibling;
  readonly attribute Node? nextSibling;
  attribute DOMString? nodeValue;
  attribute DOMString? textContent;
  boolean contains(Node? other);
  Node insertBefore(Node node, Node? child);
  Node appendChild(Node node);
  Node replaceChild(Node node, Node child);
  Node removeChild(Node child);
};

[Exposed=Window]
interface Document : Node {
  readonly attribute DOMString compatMode;
  readonly attribute Element? documentElement;
  readonly attribute HTMLElement? head;
  readonly attribute HTMLElement? body;
  readonly attribute DOMString title;
  Element? getElementById(DOMString elementId);
  sequence<Element> getElementsByTagName(DOMString qualifiedName);
  sequence<Element> getElementsByClassName(DOMString classNames);
  [NewObject] Element createElement(DOMString localName);
  [NewObject] DocumentFragment createDocumentFragment();
  [NewObject] Text createTextNode(DOMString data);
  [NewObject] Comment createComment(DOMString data);
};

[Exposed=Window]
interface DocumentType : Node {
  readonly attribute DOMString name;
  readonly attribute DOMString publicId;
  readonly attribute DOMString systemId;
};

[Exposed=Window]
interface DocumentFragment : Node {
};

[Exposed=Window]
interface Element : Node {
  readonly attribute DOMString? namespaceURI;
  readonly attribute DOMString localName;
  readonly attribute DOMString tagName;
  attribute DOMString id;
  attribute DOMString className;
  boolean hasAttributes();
  sequence<DOMString> getAttributeNames();
  DOMString? getAttribute(DOMString qualifiedName);
  undefined setAttribute(DOMString qualifiedName, DOMString value);
  undefined removeAttribute(DOMString qualifiedName);
  boolean toggleAttribute(DOMString qualifiedName, optional boolean force);
  boolean hasAttribute(DOMString qualifiedName);
  sequence<Element> getElementsByTagName(DOMString qualifiedName);
  sequence<Element> getElementsByClassName(DOMString classNames);
  readonly attribute sequence<Element> children;
  readonly attribute Element? firstElementChild;
  readonly attribute Element? lastElementChild;
  readonly attribute Element? previousElementSibling;
  readonly attribute Element? nextElementSibling;
  readonly attribute unsigned long childElementCount;
  undefined remove();
};

[Exposed=Window]
interface CharacterData : Node {
  attribute DOMString data;
  readonly attribute unsigned long length;
  undefined appendData(DOMString data);
};

[Exposed=Window]
interface Text : CharacterData {
  readonly attribute DOMString wholeText;
};

[Exposed=Window]
interface Comment : CharacterData {
};

[Exposed=Window]
interface HTMLElement : Element {
  attribute DOMString title;
  attribute DOMString lang;
  attribute boolean hidden;
};
