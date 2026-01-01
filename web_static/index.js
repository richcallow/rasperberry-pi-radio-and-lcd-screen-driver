htmx.onLoad(function () {
    document.body.addEventListener("htmx:sseBeforeMessage", function (event) {
        if (event.target.querySelector("input[type=range][data-dragging=true]") !== null) {
            event.preventDefault();
        }
    });
});