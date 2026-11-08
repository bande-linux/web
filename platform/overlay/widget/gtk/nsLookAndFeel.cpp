/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsLookAndFeel.h"

#include "mozilla/Base64.h"
#include "mozilla/StaticPtr.h"
#include "mozilla/widget/ThemeChangeKind.h"
#include "nsIURI.h"
#include "nsNetUtil.h"
#include "nsStyleSheetService.h"
#include "nsWindow.h"

using namespace mozilla;
using namespace mozilla::widget;

static BandeAppearance sAppearance;
static bool sHaveAppearance = false;
static StaticRefPtr<nsIURI> sSheet;

const BandeAppearance* nsLookAndFeel::Appearance() {
  if (!sHaveAppearance && BandeHost::IsActive()) {
    sHaveAppearance = BandeHost::GetAppearance(&sAppearance);
  }
  return sHaveAppearance ? &sAppearance : nullptr;
}

static void AppendColor(nsACString& aCss, nscolor aColor) {
  aCss.AppendPrintf("rgba(%u, %u, %u, %.4f)", NS_GET_R(aColor),
                    NS_GET_G(aColor), NS_GET_B(aColor),
                    NS_GET_A(aColor) / 255.0);
}

static void AppendVariable(nsACString& aCss, const char* aName,
                           nscolor aLight, nscolor aDark) {
  aCss.AppendPrintf("--bande-%s: light-dark(", aName);
  AppendColor(aCss, aLight);
  aCss.AppendLiteral(", ");
  AppendColor(aCss, aDark);
  aCss.AppendLiteral(");");
}

static nsCString SheetText(const BandeAppearance& aAppearance) {
  const BandePalette& l = aAppearance.light;
  const BandePalette& d = aAppearance.darkPalette;
  nsCString css(
      "@-moz-document url-prefix(\"chrome:\"), url-prefix(\"about:\") { "
      ":root {");
#define BANDE_VARIABLE(name, field) AppendVariable(css, name, l.field, d.field);
  BANDE_VARIABLE("chassis", chassis)
  BANDE_VARIABLE("frame", frame)
  BANDE_VARIABLE("sheet", sheet)
  BANDE_VARIABLE("overlay", overlay)
  BANDE_VARIABLE("panel", panel)
  BANDE_VARIABLE("well", well)
  BANDE_VARIABLE("field", field)
  BANDE_VARIABLE("raised", raised)
  BANDE_VARIABLE("hairline", hairline)
  BANDE_VARIABLE("edge", edge)
  BANDE_VARIABLE("ink", ink)
  BANDE_VARIABLE("ink2", ink2)
  BANDE_VARIABLE("ink3", ink3)
  BANDE_VARIABLE("accent", accent)
  BANDE_VARIABLE("accent-text", accentText)
  BANDE_VARIABLE("on-accent", onAccent)
  BANDE_VARIABLE("accent-soft", accentSoft)
  BANDE_VARIABLE("focus", focus)
  BANDE_VARIABLE("danger", danger)
  BANDE_VARIABLE("danger-text", dangerText)
  BANDE_VARIABLE("on-danger", onDanger)
  BANDE_VARIABLE("warning", warning)
  BANDE_VARIABLE("success", success)
  BANDE_VARIABLE("scrim", scrim)
#undef BANDE_VARIABLE
  css.AppendLiteral("} }");
  return css;
}

static void RegisterSheet(const BandeAppearance& aAppearance) {
  nsStyleSheetService* service = nsStyleSheetService::GetInstance();
  if (!service) {
    return;
  }
  nsAutoCString encoded;
  if (NS_FAILED(Base64Encode(SheetText(aAppearance), encoded))) {
    return;
  }
  nsAutoCString spec("data:text/css;base64,");
  spec.Append(encoded);
  nsCOMPtr<nsIURI> uri;
  if (NS_FAILED(NS_NewURI(getter_AddRefs(uri), spec))) {
    return;
  }
  if (sSheet) {
    service->UnregisterSheet(sSheet, nsIStyleSheetService::AGENT_SHEET);
  }
  sSheet = NS_SUCCEEDED(service->LoadAndRegisterSheet(
               uri, nsIStyleSheetService::AGENT_SHEET))
               ? uri.get()
               : nullptr;
}

void nsLookAndFeel::HandleAppearanceChanged() {
  BandeAppearance appearance;
  if (!BandeHost::GetAppearance(&appearance)) {
    return;
  }
  sAppearance = appearance;
  sHaveAppearance = true;
  RegisterSheet(appearance);
  LookAndFeel::NotifyChangedAllWindows(ThemeChangeKind::StyleAndLayout);
  nsWindow::AppearanceChanged();
}

nsresult nsLookAndFeel::NativeGetInt(IntID aID, int32_t& aResult) {
  const BandeAppearance* appearance = Appearance();
  switch (aID) {
    case IntID::CaretBlinkTime:
      aResult = 0;
      return NS_OK;
    case IntID::CaretBlinkCount:
      aResult = -1;
      return NS_OK;
    case IntID::SystemUsesDarkTheme:
      if (appearance) {
        aResult = appearance->dark ? 1 : 0;
        return NS_OK;
      }
      break;
    case IntID::PrefersReducedMotion:
      if (appearance) {
        aResult = appearance->reduceMotion ? 1 : 0;
        return NS_OK;
      }
      break;
    case IntID::PrefersReducedTransparency:
      if (appearance) {
        aResult = appearance->reduceTransparency ? 1 : 0;
        return NS_OK;
      }
      break;
    default:
      break;
  }
  return HeadlessLookAndFeel::NativeGetInt(aID, aResult);
}

nsresult nsLookAndFeel::NativeGetColor(ColorID aID, ColorScheme aScheme,
                                       nscolor& aResult) {
  const BandeAppearance* appearance = Appearance();
  if (!appearance) {
    return HeadlessLookAndFeel::NativeGetColor(aID, aScheme, aResult);
  }
  const BandePalette& p = aScheme == ColorScheme::Dark
                              ? appearance->darkPalette
                              : appearance->light;
  switch (aID) {
    case ColorID::MozHeaderbar:
    case ColorID::MozHeaderbarinactive:
    case ColorID::Activecaption:
    case ColorID::Inactivecaption:
      aResult = p.frame;
      break;
    case ColorID::MozHeaderbartext:
    case ColorID::Captiontext:
    case ColorID::Windowtext:
    case ColorID::Canvastext:
    case ColorID::MozDialogtext:
    case ColorID::Menutext:
    case ColorID::Fieldtext:
    case ColorID::MozComboboxtext:
    case ColorID::Buttontext:
    case ColorID::MozButtonhovertext:
    case ColorID::MozButtonactivetext:
    case ColorID::Infotext:
    case ColorID::MozSidebartext:
    case ColorID::MozCellhighlighttext:
    case ColorID::Highlighttext:
    case ColorID::MozColheadertext:
    case ColorID::MozColheaderhovertext:
    case ColorID::MozColheaderactivetext:
      aResult = p.ink;
      break;
    case ColorID::MozHeaderbarinactivetext:
    case ColorID::Inactivecaptiontext:
    case ColorID::Graytext:
    case ColorID::MozMenuhoverdisabled:
    case ColorID::MozMacDisabledtoolbartext:
      aResult = p.ink3;
      break;
    case ColorID::Window:
    case ColorID::Canvas:
    case ColorID::Background:
    case ColorID::Appworkspace:
    case ColorID::MozOddtreerow:
    case ColorID::MozColheader:
      aResult = p.sheet;
      break;
    case ColorID::MozDialog:
    case ColorID::MozSidebar:
    case ColorID::Threedface:
      aResult = p.chassis;
      break;
    case ColorID::Menu:
    case ColorID::Infobackground:
    case ColorID::Threedhighlight:
    case ColorID::Buttonhighlight:
      aResult = p.overlay;
      break;
    case ColorID::Field:
    case ColorID::MozCombobox:
      aResult = p.field;
      break;
    case ColorID::Buttonface:
      aResult = p.raised;
      break;
    case ColorID::MozButtonhoverface:
    case ColorID::MozCellhighlight:
    case ColorID::Threedlightshadow:
    case ColorID::MozDisabledfield:
    case ColorID::MozButtondisabledface:
    case ColorID::MozColheaderhover:
      aResult = p.well;
      break;
    case ColorID::MozButtonactiveface:
    case ColorID::MozColheaderactive:
    case ColorID::Threedshadow:
    case ColorID::Buttonshadow:
    case ColorID::MozSidebarborder:
    case ColorID::Inactiveborder:
    case ColorID::MozButtondisabledborder:
      aResult = p.hairline;
      break;
    case ColorID::Buttonborder:
    case ColorID::MozButtonhoverborder:
    case ColorID::MozButtonactiveborder:
    case ColorID::Threeddarkshadow:
    case ColorID::Activeborder:
    case ColorID::Windowframe:
      aResult = p.edge;
      break;
    case ColorID::Accentcolor:
    case ColorID::Selecteditem:
    case ColorID::MozMenuhover:
      aResult = p.accent;
      break;
    case ColorID::Accentcolortext:
    case ColorID::Selecteditemtext:
    case ColorID::MozMenuhovertext:
    case ColorID::MozMacDefaultbuttontext:
      aResult = p.onAccent;
      break;
    case ColorID::Highlight:
      aResult = p.accentSoft;
      break;
    case ColorID::MozMacFocusring:
      aResult = p.focus;
      break;
    default:
      return HeadlessLookAndFeel::NativeGetColor(aID, aScheme, aResult);
  }
  return NS_OK;
}
