[Global=Window, Exposed=Window]
interface Window : EventTarget {
  readonly attribute Window window;
  readonly attribute Window self;
  readonly attribute Document document;
};

[Exposed=Window]
namespace console {
  undefined log(any... data);
  undefined info(any... data);
  undefined warn(any... data);
  undefined error(any... data);
  undefined debug(any... data);
};
