function render(params, env)
    return "<strong>" .. (params.text or env.mainLanguage) .. "</strong>"
end
