/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "nsBandeAlertsService.h"

#include "BandeHost.h"
#include "imgIContainer.h"
#include "imgITools.h"
#include "mozilla/StaticPtr.h"
#include "nsGTKToolkit.h"
#include "nsIInputStream.h"
#include "nsNetUtil.h"
#include "nsPrintfCString.h"
#include "nsServiceManagerUtils.h"
#include "nsStreamUtils.h"

using namespace mozilla;
using mozilla::widget::BandeHost;

static nsBandeAlertsService* sAlerts = nullptr;
static constexpr auto kActionPrefix = "action:"_ns;

NS_IMPL_ISUPPORTS(nsBandeAlertsService, nsIAlertsService, nsIAlertsDoNotDisturb)

nsresult nsBandeAlertsService::Init() {
  if (!BandeHost::IsActive()) {
    return NS_ERROR_NOT_AVAILABLE;
  }
  sAlerts = this;
  return NS_OK;
}

nsBandeAlertsService::~nsBandeAlertsService() {
  if (sAlerts == this) {
    sAlerts = nullptr;
  }
}

static nsCString IconBytes(nsIAlertNotification* aAlert) {
  nsCString bytes;
  nsCOMPtr<imgIContainer> image;
  if (NS_FAILED(aAlert->GetImage(getter_AddRefs(image))) || !image) {
    return bytes;
  }
  nsCOMPtr<imgITools> tools = do_GetService("@mozilla.org/image/tools;1");
  nsCOMPtr<nsIInputStream> stream;
  if (!tools || NS_FAILED(tools->EncodeImage(image, "image/png"_ns, u""_ns,
                                             getter_AddRefs(stream))) ||
      !stream) {
    return bytes;
  }
  if (NS_FAILED(NS_ConsumeStream(stream, UINT32_MAX, bytes))) {
    bytes.Truncate();
  }
  return bytes;
}

NS_IMETHODIMP nsBandeAlertsService::ShowAlert(nsIAlertNotification* aAlert,
                                              nsIObserver* aAlertListener) {
  NS_ENSURE_ARG(aAlert);
  nsAutoString name;
  MOZ_TRY(aAlert->GetName(name));
  if (mSuppressForScreenSharing) {
    if (aAlertListener) {
      nsAutoString cookie;
      aAlert->GetCookie(cookie);
      aAlertListener->Observe(nullptr, "alertfinished", cookie.get());
    }
    return NS_OK;
  }
  nsCString id;
  if (const nsCString* known = mIds.Lookup(name).DataPtrOrNull()) {
    id = *known;
  } else {
    id = nsPrintfCString("bande-%llu", (unsigned long long)mNextId++);
  }
  nsAutoString title, text, cookie;
  MOZ_TRY(aAlert->GetTitle(title));
  MOZ_TRY(aAlert->GetText(text));
  MOZ_TRY(aAlert->GetCookie(cookie));
  bool silent = false, persistent = false;
  aAlert->GetSilent(&silent);
  aAlert->GetRequireInteraction(&persistent);
  nsTArray<RefPtr<nsIAlertAction>> actions;
  aAlert->GetActions(actions);
  nsCString buttons;
  for (const RefPtr<nsIAlertAction>& action : actions) {
    nsAutoString label, actionName;
    if (NS_FAILED(action->GetTitle(label)) ||
        NS_FAILED(action->GetAction(actionName))) {
      continue;
    }
    NS_ConvertUTF16toUTF8 labelText(label);
    labelText.ReplaceChar("\t\n", ' ');
    buttons.Append(labelText);
    buttons.Append('\t');
    buttons.Append(kActionPrefix);
    NS_ConvertUTF16toUTF8 actionText(actionName);
    actionText.ReplaceChar("\t\n", ' ');
    buttons.Append(actionText);
    buttons.Append('\n');
  }
  const nsCString icon = IconBytes(aAlert);
  if (!BandeHost::Notify(id.get(), NS_ConvertUTF16toUTF8(title).get(),
                         NS_ConvertUTF16toUTF8(text).get(),
                         reinterpret_cast<const uint8_t*>(icon.get()),
                         icon.Length(), buttons.get(), silent, persistent)) {
    return NS_ERROR_FAILURE;
  }
  if (Maybe<Shown> previous = mShown.Extract(id)) {
    if (previous->mListener) {
      previous->mListener->Observe(nullptr, "alertfinished",
                                   previous->mCookie.get());
    }
  }
  mIds.InsertOrUpdate(name, id);
  mShown.InsertOrUpdate(id, Shown{aAlert, aAlertListener, cookie, name});
  if (aAlertListener) {
    aAlertListener->Observe(nullptr, "alertshow", cookie.get());
  }
  return NS_OK;
}

void nsBandeAlertsService::Finish(const nsACString& aId) {
  Maybe<Shown> shown = mShown.Extract(aId);
  if (!shown) {
    return;
  }
  mIds.Remove(shown->mName);
  if (shown->mListener) {
    shown->mListener->Observe(nullptr, "alertfinished", shown->mCookie.get());
  }
}

NS_IMETHODIMP nsBandeAlertsService::CloseAlert(const nsAString& aAlertName,
                                               bool aContextClosed) {
  const nsCString* id = mIds.Lookup(aAlertName).DataPtrOrNull();
  if (!id) {
    return NS_OK;
  }
  const nsCString closing(*id);
  BandeHost::Unnotify(closing.get());
  Finish(closing);
  return NS_OK;
}

void nsBandeAlertsService::HandleAction(const nsACString& aId,
                                        const nsACString& aAction,
                                        const nsACString& aActivationToken) {
  RefPtr<nsBandeAlertsService> alerts = sAlerts;
  if (!alerts) {
    return;
  }
  const Shown* shown = alerts->mShown.Lookup(aId).DataPtrOrNull();
  if (!shown) {
    return;
  }
  const Shown clicked = *shown;
  if (!aActivationToken.IsEmpty()) {
    nsGTKToolkit::GetToolkit()->SetActivationToken(aActivationToken);
  }
  if (clicked.mListener) {
    if (StringBeginsWith(aAction, kActionPrefix)) {
      nsCOMPtr<nsIAlertAction> action;
      clicked.mAlert->GetAction(
          NS_ConvertUTF8toUTF16(Substring(aAction, kActionPrefix.Length())),
          getter_AddRefs(action));
      clicked.mListener->Observe(action, "alertclickcallback",
                                 clicked.mCookie.get());
    } else {
      clicked.mListener->Observe(nullptr, "alertclickcallback",
                                 clicked.mCookie.get());
    }
  }
  alerts->Finish(aId);
}

NS_IMETHODIMP nsBandeAlertsService::GetHistory(nsTArray<nsString>& aResult) {
  return NS_ERROR_NOT_AVAILABLE;
}

NS_IMETHODIMP nsBandeAlertsService::Teardown() {
  for (const auto& entry : mShown) {
    BandeHost::Unnotify(entry.GetKey().get());
  }
  mShown.Clear();
  mIds.Clear();
  return NS_OK;
}

NS_IMETHODIMP nsBandeAlertsService::PbmTeardown() {
  nsTArray<nsCString> privateIds;
  for (const auto& entry : mShown) {
    bool inPrivate = false;
    entry.GetData().mAlert->GetInPrivateBrowsing(&inPrivate);
    if (inPrivate) {
      privateIds.AppendElement(entry.GetKey());
    }
  }
  for (const nsCString& id : privateIds) {
    BandeHost::Unnotify(id.get());
    Finish(id);
  }
  return NS_OK;
}

NS_IMETHODIMP nsBandeAlertsService::IsFullscreen(bool* aRetVal) {
  *aRetVal = false;
  return NS_OK;
}

NS_IMETHODIMP nsBandeAlertsService::GetManualDoNotDisturb(bool* aRetVal) {
  return NS_ERROR_NOT_IMPLEMENTED;
}

NS_IMETHODIMP nsBandeAlertsService::SetManualDoNotDisturb(bool aDoNotDisturb) {
  return NS_ERROR_NOT_IMPLEMENTED;
}

NS_IMETHODIMP nsBandeAlertsService::GetSuppressForScreenSharing(
    bool* aRetVal) {
  NS_ENSURE_ARG(aRetVal);
  *aRetVal = mSuppressForScreenSharing;
  return NS_OK;
}

NS_IMETHODIMP nsBandeAlertsService::SetSuppressForScreenSharing(
    bool aSuppress) {
  mSuppressForScreenSharing = aSuppress;
  return NS_OK;
}
