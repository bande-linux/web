/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "Theme.h"
#include "nsIBidiKeyboard.h"
#include "nsIWidget.h"

class nsBidiKeyboard final : public nsIBidiKeyboard {
 public:
  NS_DECL_ISUPPORTS
  NS_DECL_NSIBIDIKEYBOARD

 private:
  ~nsBidiKeyboard() = default;
};

NS_IMPL_ISUPPORTS(nsBidiKeyboard, nsIBidiKeyboard)

NS_IMETHODIMP
nsBidiKeyboard::Reset() { return NS_OK; }

NS_IMETHODIMP
nsBidiKeyboard::IsLangRTL(bool* aIsRTL) { return NS_ERROR_FAILURE; }

NS_IMETHODIMP
nsBidiKeyboard::GetHaveBidiKeyboards(bool* aResult) {
  return NS_ERROR_NOT_IMPLEMENTED;
}

already_AddRefed<nsIBidiKeyboard> nsIWidget::CreateBidiKeyboardInner() {
  return do_AddRef(new nsBidiKeyboard());
}

already_AddRefed<mozilla::widget::Theme> do_CreateNativeThemeDoNotUseDirectly() {
  return do_AddRef(new mozilla::widget::Theme(
      mozilla::widget::Theme::ScrollbarStyle()));
}
