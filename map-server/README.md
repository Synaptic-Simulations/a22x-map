## map-server

The map server serves `png` images of the rendered map. 

URL format:

```
http://127.0.0.1/map.png?
```

where the query parameters are:

* `id={}`: The id of the map to render.
* `res={},{}`: The resolution of the image - width, then height.
* `pos={},{}`: The position of the center - latitude, then longitude.
* `heading={}`: The heading of the map in degrees.
* `range={}`: The range of the map in nautical miles. Valid values are 2, 5, 10, 20, 40, 80, 160, 320, and 640.
* `alt={}`: The altitude of the aircraft in feet MSL.
