/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#ifndef NativeKeyBindings_h
#define NativeKeyBindings_h

#include "mozilla/EventForwards.h"
#include "mozilla/Maybe.h"
#include "mozilla/NativeKeyBindingsType.h"
#include "nsTArray.h"

namespace mozilla {
class WritingMode;
namespace widget {

class NativeKeyBindings final {
 public:
  static void GetEditCommandsForTests(NativeKeyBindingsType aType,
                                      const WidgetKeyboardEvent& aEvent,
                                      const Maybe<WritingMode>& aWritingMode,
                                      nsTArray<CommandInt>& aCommands);
};

}  // namespace widget
}  // namespace mozilla

#endif
