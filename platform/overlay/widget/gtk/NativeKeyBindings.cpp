/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "NativeKeyBindings.h"

#include "mozilla/TextEvents.h"
#include "mozilla/WritingModes.h"

namespace mozilla::widget {

static void AppendCommand(nsTArray<CommandInt>& aCommands, Command aCommand) {
  aCommands.AppendElement(static_cast<CommandInt>(aCommand));
}

void NativeKeyBindings::GetEditCommandsForTests(
    NativeKeyBindingsType aType, const WidgetKeyboardEvent& aEvent,
    const Maybe<WritingMode>&, nsTArray<CommandInt>& aCommands) {
  const bool shift = aEvent.IsShift();
  const bool word = aEvent.IsControl();
  const bool singleLine = aType == NativeKeyBindingsType::SingleLineEditor;
  switch (aEvent.mKeyNameIndex) {
    case KEY_NAME_INDEX_Backspace:
      AppendCommand(aCommands, word ? Command::DeleteWordBackward
                                    : Command::DeleteCharBackward);
      break;
    case KEY_NAME_INDEX_Delete:
      AppendCommand(aCommands, word ? Command::DeleteWordForward
                                    : Command::DeleteCharForward);
      break;
    case KEY_NAME_INDEX_ArrowLeft:
      AppendCommand(aCommands,
                    word ? (shift ? Command::SelectWordPrevious
                                  : Command::WordPrevious)
                         : (shift ? Command::SelectCharPrevious
                                  : Command::CharPrevious));
      break;
    case KEY_NAME_INDEX_ArrowRight:
      AppendCommand(aCommands, word ? (shift ? Command::SelectWordNext
                                             : Command::WordNext)
                                    : (shift ? Command::SelectCharNext
                                             : Command::CharNext));
      break;
    case KEY_NAME_INDEX_Home:
      AppendCommand(aCommands,
                    shift ? Command::SelectBeginLine : Command::BeginLine);
      break;
    case KEY_NAME_INDEX_End:
      AppendCommand(aCommands,
                    shift ? Command::SelectEndLine : Command::EndLine);
      break;
    case KEY_NAME_INDEX_ArrowUp:
      if (!singleLine) {
        AppendCommand(aCommands, shift ? Command::SelectLinePrevious
                                       : Command::LinePrevious);
      }
      break;
    case KEY_NAME_INDEX_ArrowDown:
      if (!singleLine) {
        AppendCommand(aCommands,
                      shift ? Command::SelectLineNext : Command::LineNext);
      }
      break;
    default:
      break;
  }
}

}  // namespace mozilla::widget
